use crate::protocol::{NetMessage, PlayerInfo, PlayerTransform};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

struct PeerConnection {
    stream: TcpStream,
    id: u32,
    name: String,
    addr: String,
}

pub struct NetworkManager {
    is_hosting: AtomicBool,
    is_connected: AtomicBool,
    local_name: Mutex<String>,
    local_id: AtomicU32,
    peers: Arc<Mutex<Vec<PeerConnection>>>,
    active_client: Arc<Mutex<Option<TcpStream>>>,
    player_list: Arc<Mutex<Vec<PlayerInfo>>>,
    next_player_id: AtomicU32,
    /// Other players' transforms: pseudo → transform
    remote_transforms: Arc<Mutex<HashMap<String, PlayerTransform>>>,
    /// Our own transform (updated by hook.rs reading camera data)
    local_transform: Arc<Mutex<PlayerTransform>>,
    /// Timestamp of last transform broadcast
    last_transform_send: Arc<Mutex<Instant>>,
}

impl NetworkManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            is_hosting: AtomicBool::new(false),
            is_connected: AtomicBool::new(false),
            local_name: Mutex::new("Builder".to_string()),
            local_id: AtomicU32::new(1),
            peers: Arc::new(Mutex::new(Vec::new())),
            active_client: Arc::new(Mutex::new(None)),
            player_list: Arc::new(Mutex::new(Vec::new())),
            next_player_id: AtomicU32::new(2),
            remote_transforms: Arc::new(Mutex::new(HashMap::new())),
            local_transform: Arc::new(Mutex::new(PlayerTransform {
                pseudo: "Builder".to_string(),
                x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0,
            })),
            last_transform_send: Arc::new(Mutex::new(Instant::now())),
        })
    }

    pub fn is_active(&self) -> bool {
        self.is_hosting.load(Ordering::SeqCst) || self.is_connected.load(Ordering::SeqCst)
    }

    pub fn is_hosting(&self) -> bool {
        self.is_hosting.load(Ordering::SeqCst)
    }

    pub fn set_local_name(&self, name: String) {
        *self.local_name.lock().unwrap() = name.clone();
        self.local_transform.lock().unwrap().pseudo = name.clone();
        self.update_self_in_player_list();
    }

    pub fn get_local_name(&self) -> String {
        self.local_name.lock().unwrap().clone()
    }

    pub fn get_player_list(&self) -> Vec<PlayerInfo> {
        self.player_list.lock().unwrap().clone()
    }

    /// Update our local transform (called by hook.rs after reading camera data)
    pub fn update_local_transform(&self, x: f32, y: f32, z: f32, yaw: f32, pitch: f32) {
        let mut t = self.local_transform.lock().unwrap();
        t.x = x;
        t.y = y;
        t.z = z;
        t.yaw = yaw;
        t.pitch = pitch;
        t.pseudo = self.get_local_name();
    }

    /// Get all remote players' transforms (for rendering)
    pub fn get_remote_transforms(&self) -> Vec<PlayerTransform> {
        self.remote_transforms.lock().unwrap().values().cloned().collect()
    }

    /// Broadcast our transform if enough time has passed (~20 Hz)
    pub fn maybe_broadcast_transform(&self) {
        let now = Instant::now();
        {
            let mut last = self.last_transform_send.lock().unwrap();
            if now.duration_since(*last) < Duration::from_millis(50) {
                return;
            }
            *last = now;
        }

        let transform = self.local_transform.lock().unwrap().clone();
        self.broadcast_message(&NetMessage::TransformUpdate(transform));
    }

    fn update_self_in_player_list(&self) {
        let name = self.get_local_name();
        let id = self.local_id.load(Ordering::SeqCst);
        let is_host = self.is_hosting.load(Ordering::SeqCst);

        let mut list = self.player_list.lock().unwrap();
        if let Some(entry) = list.iter_mut().find(|p| p.id == id) {
            entry.name = name.clone();
            entry.is_host = is_host;
        } else {
            list.push(PlayerInfo { id, name, is_host });
        }
    }

    fn rebuild_player_list_and_broadcast(&self) {
        let name = self.get_local_name();
        let host_id = self.local_id.load(Ordering::SeqCst);

        let mut list = vec![PlayerInfo {
            id: host_id,
            name: name.clone(),
            is_host: true,
        }];

        if let Ok(peers) = self.peers.lock() {
            for peer in peers.iter() {
                list.push(PlayerInfo {
                    id: peer.id,
                    name: peer.name.clone(),
                    is_host: false,
                });
            }
        }

        *self.player_list.lock().unwrap() = list.clone();
        self.broadcast_message(&NetMessage::PlayerListUpdate { players: list });
    }

    /// Start a multiplayer host server on the specified port
    pub fn start_host(self: &Arc<Self>, port: u16) -> Result<(), String> {
        let bind_addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&bind_addr).map_err(|e| e.to_string())?;
        listener.set_nonblocking(false).ok();

        self.is_hosting.store(true, Ordering::SeqCst);
        self.local_id.store(1, Ordering::SeqCst);

        let host_name = self.get_local_name();
        *self.player_list.lock().unwrap() = vec![PlayerInfo {
            id: 1,
            name: host_name,
            is_host: true,
        }];

        println!("\x1b[32m[GladeSync Server] Listening on {} - Ready for players to join!\x1b[0m", bind_addr);

        let self_clone = Arc::clone(self);
        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(socket) => {
                        let peer_addr = socket.peer_addr().map(|a| a.to_string()).unwrap_or_default();
                        println!("\x1b[36m[GladeSync Server] Player connected from: {}\x1b[0m", peer_addr);

                        let assigned_id = self_clone.next_player_id.fetch_add(1, Ordering::SeqCst);

                        let ack = NetMessage::HandshakeAck {
                            assigned_id,
                            server_version: "0.1.0".to_string(),
                            host_name: self_clone.get_local_name(),
                        };
                        if let Ok(json) = serde_json::to_string(&ack) {
                            let mut s = socket.try_clone().unwrap();
                            let _ = writeln!(s, "{}", json);
                        }

                        let read_stream = match socket.try_clone() {
                            Ok(s) => s,
                            Err(_) => continue,
                        };

                        {
                            let mut peers = self_clone.peers.lock().unwrap();
                            peers.push(PeerConnection {
                                stream: socket,
                                id: assigned_id,
                                name: format!("Player {}", assigned_id),
                                addr: peer_addr.clone(),
                            });
                        }

                        let self_peer = Arc::clone(&self_clone);
                        let peer_id = assigned_id;
                        let peer_addr_clone = peer_addr.clone();
                        thread::spawn(move || {
                            let reader = BufReader::new(read_stream);
                            for line in reader.lines() {
                                match line {
                                    Ok(data) => {
                                        if let Ok(msg) = serde_json::from_str::<NetMessage>(&data) {
                                            if let NetMessage::Handshake { player_name, .. } = &msg {
                                                let mut peers = self_peer.peers.lock().unwrap();
                                                if let Some(p) = peers.iter_mut().find(|p| p.id == peer_id) {
                                                    p.name = player_name.clone();
                                                }
                                                drop(peers);
                                                self_peer.rebuild_player_list_and_broadcast();
                                            }
                                            self_peer.handle_incoming_message(msg);
                                        }
                                    }
                                    Err(_) => {
                                        println!("\x1b[33m[GladeSync Server] Player {} (id={}) disconnected.\x1b[0m", peer_addr_clone, peer_id);
                                        {
                                            let mut peers = self_peer.peers.lock().unwrap();
                                            peers.retain(|p| p.id != peer_id);
                                        }
                                        // Also remove their transform
                                        {
                                            let mut transforms = self_peer.remote_transforms.lock().unwrap();
                                            transforms.retain(|k, _| {
                                                !self_peer.peers.lock().unwrap().iter().any(|p| &p.name == k)
                                            });
                                        }
                                        self_peer.rebuild_player_list_and_broadcast();
                                        break;
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("[GladeSync Server] Accept error: {}", e);
                    }
                }
            }
        });

        Ok(())
    }

    /// Connect as a client to a host player
    pub fn connect_to_host(self: &Arc<Self>, addr: &str) -> Result<(), String> {
        println!("\x1b[33m[GladeSync Client] Connecting to host {}...\x1b[0m", addr);
        let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;

        let player_name = self.get_local_name();
        let handshake = NetMessage::Handshake {
            client_version: "0.1.0".to_string(),
            player_name,
        };
        let json = serde_json::to_string(&handshake).map_err(|e| e.to_string())?;
        writeln!(stream, "{}", json).map_err(|e| e.to_string())?;

        let stream_clone = stream.try_clone().map_err(|e| e.to_string())?;
        if let Ok(mut client_slot) = self.active_client.lock() {
            *client_slot = Some(stream);
        }

        self.is_connected.store(true, Ordering::SeqCst);

        let self_clone = Arc::clone(self);
        thread::spawn(move || {
            let reader = BufReader::new(stream_clone);
            for line in reader.lines() {
                match line {
                    Ok(data) => {
                        if let Ok(msg) = serde_json::from_str::<NetMessage>(&data) {
                            if let NetMessage::HandshakeAck { assigned_id, .. } = &msg {
                                self_clone.local_id.store(*assigned_id, Ordering::SeqCst);
                                self_clone.update_self_in_player_list();
                            }
                            if let NetMessage::PlayerListUpdate { players } = &msg {
                                *self_clone.player_list.lock().unwrap() = players.clone();
                            }
                            self_clone.handle_incoming_message(msg);
                        }
                    }
                    Err(_) => {
                        println!("\x1b[33m[GladeSync] Disconnected from host.\x1b[0m");
                        self_clone.is_connected.store(false, Ordering::SeqCst);
                        *self_clone.active_client.lock().unwrap() = None;
                        // Clear remote transforms
                        self_clone.remote_transforms.lock().unwrap().clear();
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Disconnect from current session
    pub fn disconnect(&self) {
        self.is_hosting.store(false, Ordering::SeqCst);
        self.is_connected.store(false, Ordering::SeqCst);
        *self.active_client.lock().unwrap() = None;
        self.peers.lock().unwrap().clear();
        self.remote_transforms.lock().unwrap().clear();
        *self.player_list.lock().unwrap() = vec![];
        println!("\x1b[33m[GladeSync] Disconnected.\x1b[0m");
    }

    /// Kick a player by name (host only)
    pub fn kick_player(self: &Arc<Self>, player_name: &str) -> bool {
        if !self.is_hosting.load(Ordering::SeqCst) {
            return false;
        }

        let mut peers = self.peers.lock().unwrap();
        if let Some(pos) = peers.iter().position(|p| p.name == player_name) {
            let kick_msg = NetMessage::YouKicked {
                reason: "Kicked by host".to_string(),
            };
            if let Ok(json) = serde_json::to_string(&kick_msg) {
                let _ = writeln!(peers[pos].stream, "{}", json);
            }
            peers.remove(pos);
            drop(peers);
            self.remote_transforms.lock().unwrap().remove(player_name);
            self.rebuild_player_list_and_broadcast();
            println!("\x1b[33m[GladeSync] Kicked player: {}\x1b[0m", player_name);
            return true;
        }
        false
    }

    /// Broadcast an action message to all connected peers
    pub fn broadcast_message(&self, msg: &NetMessage) {
        if let Ok(json) = serde_json::to_string(msg) {
            if let Ok(mut peers) = self.peers.lock() {
                peers.retain_mut(|peer| {
                    writeln!(peer.stream, "{}", json).is_ok()
                });
            }

            if let Ok(mut client_opt) = self.active_client.lock() {
                if let Some(ref mut client) = *client_opt {
                    let _ = writeln!(client, "{}", json);
                }
            }
        }
    }

    fn handle_incoming_message(&self, msg: NetMessage) {
        match msg {
            NetMessage::Handshake { player_name, .. } => {
                println!("\x1b[35m[GladeSync] Handshake from player: {}\x1b[0m", player_name);
            }
            NetMessage::HandshakeAck { assigned_id, host_name, .. } => {
                println!("\x1b[32m[GladeSync] Handshake accepted! Assigned ID: #{} | Host: {}\x1b[0m", assigned_id, host_name);
            }
            NetMessage::BroadcastAction(action) => {
                println!(
                    "\x1b[36m[GladeSync Action] Received remote edit: {:?} (ID: {})\x1b[0m",
                    action.edit_category, action.action_id
                );
            }
            NetMessage::CursorStream(_cursor) => {
                // High frequency cursor coords (legacy, unused for now)
            }
            NetMessage::TransformUpdate(transform) => {
                // Store the remote player's transform for rendering
                self.remote_transforms.lock().unwrap().insert(transform.pseudo.clone(), transform);
            }
            NetMessage::SyncSaveState { glade_name, .. } => {
                println!("\x1b[32m[GladeSync Sync] Received full glade snapshot: {}\x1b[0m", glade_name);
            }
            NetMessage::ChatMessage { sender, text } => {
                println!("\x1b[34m[{}] {}\x1b[0m", sender, text);
            }
            NetMessage::PlayerListUpdate { players } => {
                println!("\x1b[35m[GladeSync] Players online ({}): {}\x1b[0m",
                    players.len(),
                    players.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", "));
            }
            NetMessage::YouKicked { reason } => {
                println!("\x1b[31m[GladeSync] Kicked: {}\x1b[0m", reason);
                self.is_connected.store(false, Ordering::SeqCst);
                *self.active_client.lock().unwrap() = None;
                self.remote_transforms.lock().unwrap().clear();
            }
            NetMessage::Ping => {}
            NetMessage::Pong => {}
        }
    }
}

use crate::protocol::{NetMessage, PlayerInfo};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

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
        self.update_self_in_player_list();
    }

    pub fn get_local_name(&self) -> String {
        self.local_name.lock().unwrap().clone()
    }

    pub fn get_player_list(&self) -> Vec<PlayerInfo> {
        self.player_list.lock().unwrap().clone()
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
        let mut list = vec![PlayerInfo { id: host_id, name: name.clone(), is_host: true }];
        if let Ok(peers) = self.peers.lock() {
            for peer in peers.iter() {
                list.push(PlayerInfo { id: peer.id, name: peer.name.clone(), is_host: false });
            }
        }
        *self.player_list.lock().unwrap() = list.clone();
        self.broadcast_message(&NetMessage::PlayerListUpdate { players: list });
    }

    pub fn start_host(self: &Arc<Self>, port: u16) -> Result<(), String> {
        let bind_addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&bind_addr).map_err(|e| e.to_string())?;
        self.is_hosting.store(true, Ordering::SeqCst);
        self.local_id.store(1, Ordering::SeqCst);
        let host_name = self.get_local_name();
        *self.player_list.lock().unwrap() = vec![PlayerInfo { id: 1, name: host_name, is_host: true }];
        println!("[GladeSync Server] Listening on {}", bind_addr);

        let self_clone = Arc::clone(self);
        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(socket) => {
                        let peer_addr = socket.peer_addr().map(|a| a.to_string()).unwrap_or_default();
                        println!("[GladeSync Server] Player connected from: {}", peer_addr);
                        let assigned_id = self_clone.next_player_id.fetch_add(1, Ordering::SeqCst);
                        let ack = NetMessage::HandshakeAck {
                            assigned_id, server_version: "0.1.0".to_string(),
                            host_name: self_clone.get_local_name(),
                        };
                        if let Ok(json) = serde_json::to_string(&ack) {
                            let mut s = socket.try_clone().unwrap();
                            let _ = writeln!(s, "{}", json);
                        }
                        let read_stream = match socket.try_clone() { Ok(s) => s, Err(_) => continue };
                        {
                            let mut peers = self_clone.peers.lock().unwrap();
                            peers.push(PeerConnection {
                                stream: socket, id: assigned_id,
                                name: format!("Player {}", assigned_id), addr: peer_addr.clone(),
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
                                        println!("[GladeSync Server] Player {} (id={}) disconnected.", peer_addr_clone, peer_id);
                                        let mut peers = self_peer.peers.lock().unwrap();
                                        peers.retain(|p| p.id != peer_id);
                                        drop(peers);
                                        self_peer.rebuild_player_list_and_broadcast();
                                        break;
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => { eprintln!("[GladeSync Server] Accept error: {}", e); }
                }
            }
        });
        Ok(())
    }

    pub fn connect_to_host(self: &Arc<Self>, addr: &str) -> Result<(), String> {
        println!("[GladeSync Client] Connecting to {}...", addr);
        let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
        let player_name = self.get_local_name();
        let handshake = NetMessage::Handshake {
            client_version: "0.1.0".to_string(), player_name,
        };
        let json = serde_json::to_string(&handshake).map_err(|e| e.to_string())?;
        writeln!(stream, "{}", json).map_err(|e| e.to_string())?;
        let stream_clone = stream.try_clone().map_err(|e| e.to_string())?;
        if let Ok(mut client_slot) = self.active_client.lock() { *client_slot = Some(stream); }
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
                        println!("[GladeSync] Disconnected from host.");
                        self_clone.is_connected.store(false, Ordering::SeqCst);
                        *self_clone.active_client.lock().unwrap() = None;
                        break;
                    }
                }
            }
        });
        Ok(())
    }

    pub fn disconnect(&self) {
        self.is_hosting.store(false, Ordering::SeqCst);
        self.is_connected.store(false, Ordering::SeqCst);
        *self.active_client.lock().unwrap() = None;
        self.peers.lock().unwrap().clear();
        *self.player_list.lock().unwrap() = vec![];
        println!("[GladeSync] Disconnected.");
    }

    pub fn kick_player(self: &Arc<Self>, player_name: &str) -> bool {
        if !self.is_hosting.load(Ordering::SeqCst) { return false; }
        let mut peers = self.peers.lock().unwrap();
        if let Some(pos) = peers.iter().position(|p| p.name == player_name) {
            let kick_msg = NetMessage::YouKicked { reason: "Kicked by host".to_string() };
            if let Ok(json) = serde_json::to_string(&kick_msg) {
                let _ = writeln!(peers[pos].stream, "{}", json);
            }
            peers.remove(pos);
            drop(peers);
            self.rebuild_player_list_and_broadcast();
            println!("[GladeSync] Kicked player: {}", player_name);
            return true;
        }
        false
    }

    pub fn broadcast_message(&self, msg: &NetMessage) {
        if let Ok(json) = serde_json::to_string(msg) {
            if let Ok(mut peers) = self.peers.lock() {
                peers.retain_mut(|peer| writeln!(peer.stream, "{}", json).is_ok());
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
                println!("[GladeSync] Handshake from: {}", player_name);
            }
            NetMessage::HandshakeAck { assigned_id, host_name, .. } => {
                println!("[GladeSync] Connected! ID: #{} | Host: {}", assigned_id, host_name);
            }
            NetMessage::BroadcastAction(action) => {
                println!("[GladeSync] Remote edit: {:?} (ID: {})", action.edit_category, action.action_id);
            }
            NetMessage::CursorStream(_) => {}
            NetMessage::SyncSaveState { glade_name, .. } => {
                println!("[GladeSync] Received glade snapshot: {}", glade_name);
            }
            NetMessage::ChatMessage { sender, text } => {
                println!("[{}] {}", sender, text);
            }
            NetMessage::PlayerListUpdate { players } => {
                println!("[GladeSync] Players online ({}): {}",
                    players.len(),
                    players.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", "));
            }
            NetMessage::YouKicked { reason } => {
                println!("[GladeSync] Kicked: {}", reason);
                self.is_connected.store(false, Ordering::SeqCst);
                *self.active_client.lock().unwrap() = None;
            }
            NetMessage::Ping => {}
            NetMessage::Pong => {}
        }
    }
}

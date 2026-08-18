use crate::protocol::{NetMessage, PlayerInfo};
use base64::Engine;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use socket2::{Socket, Domain, Type};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

struct PeerConnection {
    stream: TcpStream,
    id: u32,
    name: String,
    addr: String,
}

pub struct NetworkManager {
    is_hosting: AtomicBool,
    is_connected: AtomicBool,
    banned_addrs: Mutex<Vec<String>>,
    room_pin: Mutex<String>,
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
            banned_addrs: Mutex::new(Vec::new()),
            room_pin: Mutex::new(String::new()),
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

    pub fn set_room_pin(&self, pin: String) {
        *self.room_pin.lock().unwrap() = pin;
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

    pub fn start_host(self: &Arc<Self>, port: u16, pin: String) -> Result<(), String> {
        *self.room_pin.lock().unwrap() = pin;
        let bind_addr = format!("0.0.0.0:{}", port);
        let socket = Socket::new(Domain::IPV4, Type::STREAM, None)
            .map_err(|e| e.to_string())?;
        socket.set_reuse_address(true).map_err(|e| e.to_string())?;
        let addr: std::net::SocketAddr = bind_addr.parse()
            .map_err(|e: std::net::AddrParseError| e.to_string())?;
        socket.bind(&addr.into()).map_err(|e| e.to_string())?;
        socket.listen(128).map_err(|e| e.to_string())?;
        let listener: TcpListener = socket.into();
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;

        self.is_hosting.store(true, Ordering::SeqCst);
        self.local_id.store(1, Ordering::SeqCst);
        let host_name = self.get_local_name();
        *self.player_list.lock().unwrap() = vec![PlayerInfo { id: 1, name: host_name, is_host: true }];
        crate::debug_log!("[GladeSync Server] Listening on {}", bind_addr);

        let self_clone = Arc::clone(self);
        thread::spawn(move || {
            loop {
                if !self_clone.is_hosting.load(Ordering::SeqCst) {
                    crate::debug_log!("[GladeSync Server] Stop requested — closing listener on {}", bind_addr);
                    break;
                }
                match listener.accept() {
                    Ok((mut socket, peer_addr_sock)) => {
                        let _ = socket.set_nonblocking(false);
                        let peer_addr = peer_addr_sock.to_string();
                        let peer_ip = peer_addr_sock.ip().to_string();
                        if self_clone.banned_addrs.lock().unwrap().iter().any(|a| *a == peer_ip) {
                            crate::debug_log!("[GladeSync Server] Rejected banned IP: {}", peer_ip);
                            drop(socket);
                            continue;
                        }
                        crate::debug_log!("[GladeSync Server] Player connected from: {}", peer_addr);

                        // ── Read handshake first to check PIN ──
                        let read_stream = match socket.try_clone() { Ok(s) => s, Err(_) => continue };
                        let _ = read_stream.set_read_timeout(Some(Duration::from_secs(5)));
                        let mut reader = BufReader::new(read_stream);
                        let mut hs_line = String::new();
                        let player_name: String;
                        match reader.read_line(&mut hs_line) {
                            Ok(0) | Err(_) => {
                                crate::debug_log!("[GladeSync Server] Player {} disconnected before handshake", peer_addr);
                                continue;
                            }
                            Ok(_) => {
                                match serde_json::from_str::<NetMessage>(hs_line.trim()) {
                                    Ok(NetMessage::Handshake { player_name: pn, pin, .. }) => {
                                        let expected = self_clone.room_pin.lock().unwrap().clone();
                                        if !expected.is_empty() && pin != expected {
                                            crate::debug_log!("[GladeSync Server] Wrong PIN from {}", peer_addr);
                                            let reject = NetMessage::HandshakeReject { reason: "Wrong PIN".to_string() };
                                            if let Ok(rj) = serde_json::to_string(&reject) {
                                                let _ = writeln!(socket, "{}", rj);
                                            }
                                            drop(socket);
                                            continue;
                                        }
                                        player_name = pn;
                                    }
                                    _ => {
                                        crate::debug_log!("[GladeSync Server] Invalid handshake from {}", peer_addr);
                                        drop(socket);
                                        continue;
                                    }
                                }
                            }
                        }
                        // Reset read timeout to none for ongoing reads
                        let _ = reader.get_ref().set_read_timeout(None);

                        let assigned_id = self_clone.next_player_id.fetch_add(1, Ordering::SeqCst);
                        let ack = NetMessage::HandshakeAck {
                            assigned_id, server_version: "0.1.0".to_string(),
                            host_name: self_clone.get_local_name(),
                        };
                        if let Ok(json) = serde_json::to_string(&ack) {
                            let _ = writeln!(socket, "{}", json);
                        }
                        {
                            let mut peers = self_clone.peers.lock().unwrap();
                            peers.push(PeerConnection {
                                stream: socket, id: assigned_id,
                                name: player_name.clone(), addr: peer_addr.clone(),
                            });
                        }
                        self_clone.rebuild_player_list_and_broadcast();

                        // ── Send save state to new client ──
                        if let Some((save_name, save_bytes)) = crate::hook::read_latest_save() {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&save_bytes);
                            let sync_msg = NetMessage::SyncSaveState {
                                glade_name: save_name,
                                save_bytes_base64: b64,
                            };
                            if let Ok(json) = serde_json::to_string(&sync_msg) {
                                let mut peers = self_clone.peers.lock().unwrap();
                                if let Some(p) = peers.iter_mut().find(|p| p.id == assigned_id) {
                                    let _ = writeln!(p.stream, "{}", json);
                                    crate::debug_log!("[GladeSync Server] Sent save state to player {}", assigned_id);
                                }
                            }
                        } else {
                            crate::debug_log!("[GladeSync Server] No save file found to sync");
                        }

                        let self_peer = Arc::clone(&self_clone);
                        let peer_id = assigned_id;
                        let peer_addr_clone = peer_addr.clone();
                        thread::spawn(move || {
                            for line in reader.lines() {
                                match line {
                                    Ok(data) => {
                                        if let Ok(msg) = serde_json::from_str::<NetMessage>(&data) {
                                            self_peer.handle_incoming_message(msg);
                                        }
                                    }
                                    Err(_) => {
                                        crate::debug_log!("[GladeSync Server] Player {} (id={}) disconnected.", peer_addr_clone, peer_id);
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
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        crate::debug_log!("[GladeSync ERROR] Server accept error: {}", e);
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        });
        Ok(())
    }

    pub fn connect_to_host(self: &Arc<Self>, addr: &str, pin: String) -> Result<(), String> {
        crate::debug_log!("[GladeSync Client] Connecting to {}...", addr);
        let socket_addr: std::net::SocketAddr = addr
            .parse()
            .map_err(|_| "Invalid address".to_string())?;
        let mut stream = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(5))
            .map_err(|e| format!("Could not reach host: {}", e))?;

        // ── Clear stale player list before connecting ──
        // set_local_name may have been called before connect, adding a stale
        // entry with the default id=1. Clear everything so the server's
        // PlayerListUpdate is the single source of truth.
        *self.player_list.lock().unwrap() = vec![];

        let player_name = self.get_local_name();
        let handshake = NetMessage::Handshake {
            client_version: "0.1.0".to_string(), player_name, pin,
        };
        let json = serde_json::to_string(&handshake).map_err(|e| e.to_string())?;
        writeln!(stream, "{}", json).map_err(|e| e.to_string())?;

        // ── Read HandshakeAck ──
        // We use a single BufReader that is REUSED by the receive thread.
        // Previously, a separate BufReader for the ack would over-read extra
        // bytes (like the first PlayerListUpdate) into its internal buffer,
        // and those bytes were lost when the BufReader was dropped — causing
        // the client to miss the first player list update and show a stale
        // duplicate of itself.
        stream.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
        let read_stream = stream.try_clone().map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(read_stream);

        let mut line = String::new();
        let assigned_id: u32;
        let host_name_recv: String;

        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => return Err("Host closed the connection — session may not exist.".to_string()),
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() { continue; }
                    match serde_json::from_str::<NetMessage>(trimmed) {
                        Ok(NetMessage::HandshakeAck { assigned_id: id, host_name, .. }) => {
                            assigned_id = id;
                            host_name_recv = host_name;
                            break;
                        }
                        Ok(NetMessage::HandshakeReject { reason }) => {
                            return Err(format!("Rejected: {}", reason));
                        }
                        _ => continue,
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                    return Err("No response from host — the session doesn't exist or is unreachable.".to_string());
                }
                Err(e) => return Err(format!("Connection error: {}", e)),
            }
        }

        // Reset to blocking with no timeout for normal ongoing play.
        stream.set_read_timeout(None).map_err(|e| e.to_string())?;

        // Store the original stream for writing (broadcast_message uses it).
        if let Ok(mut client_slot) = self.active_client.lock() { *client_slot = Some(stream); }
        self.is_connected.store(true, Ordering::SeqCst);
        self.local_id.store(assigned_id, Ordering::SeqCst);
        self.update_self_in_player_list();
        crate::debug_log!("[GladeSync] Connected! ID: #{} | Host: {}", assigned_id, host_name_recv);

        // ── Receive thread ──
        // Reuses the same BufReader (with any buffered data preserved) so no
        // messages from the server are lost.
        let self_clone = Arc::clone(self);
        thread::spawn(move || {
            for line in reader.lines() {
                match line {
                    Ok(data) => {
                        if let Ok(msg) = serde_json::from_str::<NetMessage>(&data) {
                            // PlayerListUpdate: replace the entire list (server is
                            // the single source of truth — no local additions).
                            if let NetMessage::PlayerListUpdate { players } = &msg {
                                *self_clone.player_list.lock().unwrap() = players.clone();
                            }
                            // SyncSaveState: write the received save file to disk.
                            if let NetMessage::SyncSaveState { glade_name, save_bytes_base64 } = &msg {
                                if let Ok(bytes) = base64::engine::general_purpose::STANDARD
                                    .decode(save_bytes_base64)
                                {
                                    if crate::hook::write_save_file(glade_name, &bytes) {
                                        crate::debug_log!("[GladeSync] Save state received and written: {}", glade_name);
                                        // Auto-reload: wait briefly for the file write to settle,
                                        // then simulate a quick-load keypress so the game picks
                                        // up the new save without manual intervention.
                                        thread::spawn(move || {
                                            thread::sleep(Duration::from_millis(500));
                                            crate::hook::trigger_reload();
                                            crate::debug_log!("[GladeSync] Auto-reload triggered");
                                        });
                                    } else {
                                        crate::debug_log!("[GladeSync ERROR] Failed to write save state: {}", glade_name);
                                    }
                                }
                            }
                            self_clone.handle_incoming_message(msg);
                        }
                    }
                    Err(_) => {
                        crate::debug_log!("[GladeSync] Disconnected from host.");
                        self_clone.is_connected.store(false, Ordering::SeqCst);
                        *self_clone.active_client.lock().unwrap() = None;
                        *self_clone.player_list.lock().unwrap() = vec![];
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
        self.local_id.store(1, Ordering::SeqCst);
        *self.active_client.lock().unwrap() = None;
        self.peers.lock().unwrap().clear();
        *self.player_list.lock().unwrap() = vec![];
        self.banned_addrs.lock().unwrap().clear();
        crate::debug_log!("[GladeSync] Disconnected.");
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
            crate::debug_log!("[GladeSync] Kicked player: {}", player_name);
            return true;
        }
        false
    }

    pub fn ban_player(self: &Arc<Self>, player_name: &str) -> bool {
        if !self.is_hosting.load(Ordering::SeqCst) { return false; }
        let mut peers = self.peers.lock().unwrap();
        if let Some(pos) = peers.iter().position(|p| p.name == player_name) {
            let peer_addr = peers[pos].addr.clone();
            let peer_ip: String = peer_addr.split(':').next().unwrap_or("").to_string();
            let ban_msg = NetMessage::YouBanned { reason: "Banned by host".to_string() };
            if let Ok(json) = serde_json::to_string(&ban_msg) {
                let _ = writeln!(peers[pos].stream, "{}", json);
            }
            peers.remove(pos);
            drop(peers);
            self.banned_addrs.lock().unwrap().push(peer_ip.clone());
            self.rebuild_player_list_and_broadcast();
            crate::debug_log!("[GladeSync] Banned player: {} ({})", player_name, peer_ip);
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
                crate::debug_log!("[GladeSync] Handshake from: {}", player_name);
            }
            NetMessage::HandshakeReject { reason } => {
                crate::debug_log!("[GladeSync] Rejected by host: {}", reason);
            }
            NetMessage::HandshakeAck { assigned_id, host_name, .. } => {
                crate::debug_log!("[GladeSync] Connected! ID: #{} | Host: {}", assigned_id, host_name);
            }
            NetMessage::BroadcastAction(action) => {
                crate::debug_log!("[GladeSync] Remote edit: {:?} (ID: {})", action.edit_category, action.action_id);
            }
            NetMessage::CursorStream(_) => {}
            NetMessage::SyncSaveState { glade_name, .. } => {
                crate::debug_log!("[GladeSync] Received glade snapshot: {}", glade_name);
            }
            NetMessage::ChatMessage { sender, text } => {
                println!("[{}] {}", sender, text);
            }
            NetMessage::PlayerListUpdate { players } => {
                crate::debug_log!("[GladeSync] Players online ({}): {}",
                    players.len(),
                    players.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", "));
            }
            NetMessage::YouKicked { reason } => {
                crate::debug_log!("[GladeSync] Kicked: {}", reason);
                self.is_connected.store(false, Ordering::SeqCst);
                *self.active_client.lock().unwrap() = None;
                *self.player_list.lock().unwrap() = vec![];
            }
            NetMessage::YouBanned { reason } => {
                crate::debug_log!("[GladeSync] Banned: {}", reason);
                self.is_connected.store(false, Ordering::SeqCst);
                *self.active_client.lock().unwrap() = None;
                *self.player_list.lock().unwrap() = vec![];
            }
            NetMessage::Ping => {}
            NetMessage::Pong => {}
        }
    }
}

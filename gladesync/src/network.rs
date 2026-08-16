use crate::protocol::NetMessage;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct NetworkManager {
    is_hosting: AtomicBool,
    is_connected: AtomicBool,
    peers: Arc<Mutex<Vec<TcpStream>>>,
    active_client: Arc<Mutex<Option<TcpStream>>>,
}

impl NetworkManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            is_hosting: AtomicBool::new(false),
            is_connected: AtomicBool::new(false),
            peers: Arc::new(Mutex::new(Vec::new())),
            active_client: Arc::new(Mutex::new(None)),
        })
    }

    pub fn is_active(&self) -> bool {
        self.is_hosting.load(Ordering::SeqCst) || self.is_connected.load(Ordering::SeqCst)
    }

    /// Start a multiplayer host server on the specified port
    pub fn start_host(self: &Arc<Self>, port: u16) -> Result<(), String> {
        let bind_addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&bind_addr).map_err(|e| e.to_string())?;
        listener.set_nonblocking(false).ok();

        self.is_hosting.store(true, Ordering::SeqCst);
        println!("\x1b[32m[GladeSync Server] Listening on {} - Ready for players to join!\x1b[0m", bind_addr);

        let self_clone = Arc::clone(self);
        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(mut socket) => {
                        let peer_addr = socket.peer_addr().map(|a| a.to_string()).unwrap_or_default();
                        println!("\x1b[36m[GladeSync Server] Player connected from: {}\x1b[0m", peer_addr);

                        // Send Handshake ACK
                        let ack = NetMessage::HandshakeAck {
                            assigned_id: 2,
                            server_version: "0.1.0".to_string(),
                        };
                        if let Ok(json) = serde_json::to_string(&ack) {
                            let _ = writeln!(socket, "{}", json);
                        }

                        if let Ok(mut peers) = self_clone.peers.lock() {
                            if let Ok(clone_sock) = socket.try_clone() {
                                peers.push(clone_sock);
                            }
                        }

                        // Spawn receiver for this peer
                        let self_peer = Arc::clone(&self_clone);
                        thread::spawn(move || {
                            let reader = BufReader::new(socket);
                            for line in reader.lines() {
                                match line {
                                    Ok(data) => {
                                        if let Ok(msg) = serde_json::from_str::<NetMessage>(&data) {
                                            self_peer.handle_incoming_message(msg);
                                        }
                                    }
                                    Err(_) => {
                                        println!("\x1b[33m[GladeSync Server] Player {} disconnected.\x1b[0m", peer_addr);
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

        // Send handshake
        let handshake = NetMessage::Handshake {
            client_version: "0.1.0".to_string(),
            player_name: "Guest Builder".to_string(),
        };
        let json = serde_json::to_string(&handshake).map_err(|e| e.to_string())?;
        writeln!(stream, "{}", json).map_err(|e| e.to_string())?;

        let stream_clone = stream.try_clone().map_err(|e| e.to_string())?;
        if let Ok(mut client_slot) = self.active_client.lock() {
            *client_slot = Some(stream);
        }

        self.is_connected.store(true, Ordering::SeqCst);
        println!("\x1b[32m[GladeSync Client] Connected successfully to host {}!\x1b[0m", addr);

        let self_clone = Arc::clone(self);
        thread::spawn(move || {
            let reader = BufReader::new(stream_clone);
            for line in reader.lines() {
                match line {
                    Ok(data) => {
                        if let Ok(msg) = serde_json::from_str::<NetMessage>(&data) {
                            self_clone.handle_incoming_message(msg);
                        }
                    }
                    Err(_) => {
                        println!("\x1b[31m[GladeSync Client] Disconnected from host.\x1b[0m");
                        self_clone.is_connected.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Broadcast an action message to all connected peers
    pub fn broadcast_message(&self, msg: &NetMessage) {
        if let Ok(json) = serde_json::to_string(msg) {
            // If hosting, send to all connected peers
            if let Ok(mut peers) = self.peers.lock() {
                peers.retain_mut(|socket| {
                    writeln!(socket, "{}", json).is_ok()
                });
            }

            // If client, send to host
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
            NetMessage::HandshakeAck { assigned_id, .. } => {
                println!("\x1b[32m[GladeSync] Handshake accepted! Assigned ID: #{}\x1b[0m", assigned_id);
            }
            NetMessage::BroadcastAction(action) => {
                println!(
                    "\x1b[36m[GladeSync Action] Received remote edit: {:?} (ID: {})\x1b[0m",
                    action.edit_category, action.action_id
                );
            }
            NetMessage::CursorStream(_cursor) => {
                // High frequency cursor coords
            }
            NetMessage::SyncSaveState { glade_name, .. } => {
                println!("\x1b[32m[GladeSync Sync] Received full glade snapshot: {}\x1b[0m", glade_name);
            }
            NetMessage::ChatMessage { sender, text } => {
                println!("\x1b[34m[{}] {}\x1b[0m", sender, text);
            }
            NetMessage::Ping => {}
            NetMessage::Pong => {}
        }
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditCategory {
    Wall, Roof, Terrain, Path, Garden, Water, Stairs,
    Decorator, AutoClutter, Color, Undo, Redo, Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector3D { pub x: f32, pub y: f32, pub z: f32 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorState {
    pub position: Vector3D,
    pub camera_position: Vector3D,
    pub active_tool: String,
    pub is_clicking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPayload {
    pub edit_category: EditCategory,
    pub action_id: u64,
    pub timestamp: u64,
    pub data_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub id: u32,
    pub name: String,
    pub is_host: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetMessage {
    Handshake { client_version: String, player_name: String, #[serde(default)] pin: String },
    HandshakeReject { reason: String },
    HandshakeAck { assigned_id: u32, server_version: String, host_name: String },
    SyncSaveState { glade_name: String, save_bytes_base64: String },
    BroadcastAction(ActionPayload),
    CursorStream(CursorState),
    ChatMessage { sender: String, text: String },
    PlayerListUpdate { players: Vec<PlayerInfo> },
    YouKicked { reason: String },
    YouBanned { reason: String },
    Ping,
    Pong,
}

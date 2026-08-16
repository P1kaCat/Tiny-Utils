use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditCategory {
    Wall,
    Roof,
    Terrain,
    Path,
    Garden,
    Water,
    Stairs,
    Decorator,
    AutoClutter,
    Color,
    Undo,
    Redo,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

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

/// Player transform: position + look direction in world space.
/// Sent at ~20 Hz to all connected players for real-time avatar rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerTransform {
    pub pseudo: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Yaw (horizontal look direction) in radians
    pub yaw: f32,
    /// Pitch (vertical look direction) in radians
    pub pitch: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetMessage {
    /// Initial connection handshake
    Handshake {
        client_version: String,
        player_name: String,
    },
    /// Acknowledgment and player ID assignment
    HandshakeAck {
        assigned_id: u32,
        server_version: String,
        host_name: String,
    },
    /// Full glade save state synchronization
    SyncSaveState {
        glade_name: String,
        save_bytes_base64: String,
    },
    /// Live edit action broadcast (walls, roofs, paths, etc.)
    BroadcastAction(ActionPayload),
    /// Real-time cursor/camera coordinate streaming
    CursorStream(CursorState),
    /// Chat message or notification
    ChatMessage {
        sender: String,
        text: String,
    },
    /// Player list update (host → all clients)
    PlayerListUpdate {
        players: Vec<PlayerInfo>,
    },
    /// You were kicked by the host
    YouKicked {
        reason: String,
    },
    /// Real-time player position + look direction update (~20 Hz)
    TransformUpdate(PlayerTransform),
    /// Keep-alive heartbeat
    Ping,
    Pong,
}

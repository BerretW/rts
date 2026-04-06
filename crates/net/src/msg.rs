//! Definice všech zpráv protokolu.

use serde::{Deserialize, Serialize};

// ── Clientbound (server → klient) ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    // Handshake
    Hello       { protocol: u32, server_name: String, player_id: u64 },
    Error       { msg: String },

    // Lobby list
    LobbyList   { lobbies: Vec<LobbyInfo> },

    // Stav lobby
    LobbyJoined { lobby: LobbyState },
    LobbyUpdated{ lobby: LobbyState },
    LobbyLeft,

    // Chat
    ChatMsg     { from: String, text: String },

    // Hra
    GameStart   {
        map_id:    String,
        your_team: u8,
        tick_rate: u8,
        /// Dlaždice mapy (1 byte = TileKind: 0=Grass,1=Dirt,2=Water,3=DeepWater,4=Forest,5=Rock,6=Sand,7=Bridge)
        map_tiles: Vec<u8>,
        map_w:     u32,
        map_h:     u32,
        /// Střed základny tohoto hráče (pro počáteční polohu kamery)
        base_x:    f32,
        base_y:    f32,
    },
    GameState   { tick: u64, entities: Vec<EntitySnapshot> },
    GameOver    { winner_team: Option<u8>, reason: String },
}

// ── Serverbound (klient → server) ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    // Handshake
    Hello           { protocol: u32, player_name: String },

    // Lobby
    RequestLobbyList,
    CreateLobby     { name: String, max_players: u8, map_id: String },
    JoinLobby       { id: u64 },
    LeaveLobby,
    SetReady        { ready: bool },
    StartGame,                      // pouze host

    // Chat
    ChatMsg         { text: String },

    // Herní vstup
    PlayerInput     { tick: u64, actions: Vec<PlayerAction> },
}

// ── Data ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyInfo {
    pub id:          u64,
    pub name:        String,
    pub map_id:      String,
    pub players:     u8,
    pub max_players: u8,
    pub in_game:     bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyState {
    pub id:          u64,
    pub name:        String,
    pub map_id:      String,
    pub max_players: u8,
    pub players:     Vec<PlayerInfo>,
    pub host_id:     u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub id:    u64,
    pub name:  String,
    pub team:  u8,
    pub ready: bool,
}

/// Snapshot entity pro přenos stavu hry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub id:     u64,
    pub x:      f32,
    pub y:      f32,
    pub hp:     i32,
    pub hp_max: i32,
    pub team:   u8,
    pub kind:   String,
}

/// Herní akce hráče v jednom ticku.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PlayerAction {
    MoveUnits    { unit_ids: Vec<u64>, target_x: f32, target_y: f32 },
    AttackUnit   { attacker_ids: Vec<u64>, target_id: u64 },
    StopUnits    { unit_ids: Vec<u64> },
    TrainUnit    { building_id: u64, kind_id: String },
    SpawnUnit    { kind_id: String, x: f32, y: f32 },
}

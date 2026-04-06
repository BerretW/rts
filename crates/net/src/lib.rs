//! Sdílené síťové typy a kodek pro RTS klient/server.
//!
//! Protokol: délkový prefix (4 B big-endian u32) + JSON.
//! Volitelná feature `async` aktivuje asynchronní read/write helpers
//! postavené na tokio::io.

pub mod msg;
pub mod codec;

pub use msg::{
    ServerMsg, ClientMsg,
    LobbyInfo, LobbyState, PlayerInfo, EntitySnapshot, PlayerAction,
};
pub use codec::{encode_msg, decode_msg};

#[cfg(feature = "async")]
pub use codec::{write_msg, read_msg};

/// Výchozí port serveru.
pub const DEFAULT_PORT: u16 = 7777;

/// Verze protokolu – klient a server musí souhlasit.
pub const PROTOCOL_VERSION: u32 = 1;

/// Max. velikost jedné zprávy (10 MiB).
pub const MAX_MSG_SIZE: usize = 10 * 1024 * 1024;

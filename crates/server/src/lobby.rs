//! Správce lobby – vytváření, připojování, spouštění her.

use std::collections::HashMap;
use std::path::PathBuf;

use net::{LobbyInfo, LobbyState, PlayerAction, PlayerInfo, ServerMsg};

use crate::client_conn::ClientHandle;
use crate::game_session::{GameSession, GameSessionHandle};

// ── LobbyManager ─────────────────────────────────────────────────────────────

pub struct LobbyManager {
    pub scripts_dir: PathBuf,
    pub assets_dir:  PathBuf,
    pub lobbies:     HashMap<u64, Lobby>,
    next_id:         u64,
}

impl LobbyManager {
    pub fn new(scripts_dir: PathBuf, assets_dir: PathBuf) -> Self {
        Self {
            scripts_dir,
            assets_dir,
            lobbies: HashMap::new(),
            next_id: 1,
        }
    }

    // ── Příkazy z klientů ─────────────────────────────────────────────────

    pub fn lobby_list(&self) -> Vec<LobbyInfo> {
        self.lobbies.values().map(Lobby::info).collect()
    }

    pub fn create_lobby(
        &mut self,
        host: &ClientHandle,
        name: String,
        max_players: u8,
        map_id: String,
    ) -> LobbyState {
        let id = self.next_id;
        self.next_id += 1;

        let mut lobby = Lobby {
            id,
            name,
            map_id,
            max_players,
            players: HashMap::new(),
            host_id: host.id,
            game: None,
        };
        lobby.players.insert(host.id, LobbyPlayer {
            info:   PlayerInfo { id: host.id, name: host.name.clone(), team: 0, ready: false },
            handle: host.clone(),
        });

        let state = lobby.state();
        self.lobbies.insert(id, lobby);
        state
    }

    pub fn join_lobby(
        &mut self,
        lobby_id: u64,
        client: &ClientHandle,
    ) -> Result<LobbyState, String> {
        let lobby = self.lobbies.get_mut(&lobby_id)
            .ok_or_else(|| "Lobby nenalezeno".to_string())?;

        if lobby.players.len() as u8 >= lobby.max_players {
            return Err("Lobby je plné".into());
        }
        if lobby.game.is_some() {
            return Err("Hra již probíhá".into());
        }

        let team = lobby.players.len() as u8;
        lobby.players.insert(client.id, LobbyPlayer {
            info:   PlayerInfo { id: client.id, name: client.name.clone(), team, ready: false },
            handle: client.clone(),
        });

        Ok(lobby.state())
    }

    pub fn leave_lobby(&mut self, client_id: u64) {
        let to_remove: Vec<u64> = self.lobbies.iter()
            .filter_map(|(lid, l)| if l.players.contains_key(&client_id) { Some(*lid) } else { None })
            .collect();
        for lid in to_remove {
            if let Some(lobby) = self.lobbies.get_mut(&lid) {
                lobby.players.remove(&client_id);
                // Broadcast aktualizaci zbývajícím
                let state = lobby.state();
                for p in lobby.players.values() {
                    let _ = p.handle.tx.send(ServerMsg::LobbyUpdated { lobby: state.clone() });
                }
                // Zruš prázdné lobby
                if lobby.players.is_empty() {
                    self.lobbies.remove(&lid);
                    break;
                }
                // Předej hostování, pokud odešel host
                if lobby.host_id == client_id {
                    if let Some(new_host) = lobby.players.keys().next().copied() {
                        lobby.host_id = new_host;
                    }
                }
            }
        }
    }

    pub fn set_ready(&mut self, client_id: u64, ready: bool) {
        for lobby in self.lobbies.values_mut() {
            if let Some(p) = lobby.players.get_mut(&client_id) {
                p.info.ready = ready;
                let state = lobby.state();
                for pl in lobby.players.values() {
                    let _ = pl.handle.tx.send(ServerMsg::LobbyUpdated { lobby: state.clone() });
                }
                break;
            }
        }
    }

    /// Spustí hru (pouze host smí volat).
    pub fn start_game(
        &mut self,
        host_id: u64,
        scripts_dir: PathBuf,
        assets_dir:  PathBuf,
    ) -> Result<(), String> {
        let lobby = self.lobbies.values_mut()
            .find(|l| l.host_id == host_id && l.players.contains_key(&host_id))
            .ok_or_else(|| "Nejste host žádného lobby".to_string())?;

        if lobby.game.is_some() {
            return Err("Hra již probíhá".into());
        }

        let players: Vec<LobbyPlayer> = lobby.players.values().cloned().collect();
        let map_id  = lobby.map_id.clone();

        // Spustí GameSession v pozadí
        let session = GameSession::start(players, map_id, scripts_dir, assets_dir);
        lobby.game = Some(session);
        Ok(())
    }

    /// Broadcastne zprávu všem klientům ve všech lobby.
    pub fn broadcast_all(&self, msg: ServerMsg) {
        for lobby in self.lobbies.values() {
            for p in lobby.players.values() {
                let _ = p.handle.tx.send(msg.clone());
            }
        }
    }

    /// Přepošle herní vstup do aktivní game session daného klienta.
    pub fn deliver_input(&self, client_id: u64, tick: u64, actions: Vec<PlayerAction>) {
        for lobby in self.lobbies.values() {
            if lobby.players.contains_key(&client_id) {
                if let Some(game) = &lobby.game {
                    game.send_input(client_id, tick, actions);
                    return;
                }
            }
        }
    }
}

// ── Lobby ─────────────────────────────────────────────────────────────────────

pub struct Lobby {
    pub id:          u64,
    pub name:        String,
    pub map_id:      String,
    pub max_players: u8,
    pub players:     HashMap<u64, LobbyPlayer>,
    pub host_id:     u64,
    pub game:        Option<GameSessionHandle>,
}

impl Lobby {
    fn info(&self) -> LobbyInfo {
        LobbyInfo {
            id:          self.id,
            name:        self.name.clone(),
            map_id:      self.map_id.clone(),
            players:     self.players.len() as u8,
            max_players: self.max_players,
            in_game:     self.game.is_some(),
        }
    }

    fn state(&self) -> LobbyState {
        let mut players: Vec<PlayerInfo> = self.players.values()
            .map(|p| p.info.clone())
            .collect();
        players.sort_by_key(|p| p.id);
        LobbyState {
            id:          self.id,
            name:        self.name.clone(),
            map_id:      self.map_id.clone(),
            max_players: self.max_players,
            players,
            host_id:     self.host_id,
        }
    }
}

#[derive(Clone)]
pub struct LobbyPlayer {
    pub info:   PlayerInfo,
    pub handle: ClientHandle,
}

//! Per-klient síťová smyčka.

use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};

use net::{ClientMsg, ServerMsg, DEFAULT_PORT, PROTOCOL_VERSION};
use net::{read_msg, write_msg};

use crate::lobby::LobbyManager;

// ── ClientHandle ─────────────────────────────────────────────────────────────

/// Klon-abilní handle pro posílání zpráv klientovi.
#[derive(Clone)]
pub struct ClientHandle {
    pub id:   u64,
    pub name: String,
    /// Kanál do odesílací smyčky klienta.
    pub tx:   mpsc::UnboundedSender<ServerMsg>,
}

// ── Hlavní smyčka klienta ─────────────────────────────────────────────────────

pub async fn run(
    id:     u64,
    stream: TcpStream,
    mgr:    Arc<Mutex<LobbyManager>>,
) -> std::io::Result<()> {
    stream.set_nodelay(true)?;
    let (mut reader, mut writer) = tokio::io::split(stream);

    // Kanál pro posílání zpráv do writeru
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMsg>();

    // Handshake – první zpráva musí být ClientMsg::Hello
    let hello: ClientMsg = read_msg(&mut reader).await?;
    let player_name = match hello {
        ClientMsg::Hello { protocol, player_name } if protocol == PROTOCOL_VERSION => player_name,
        ClientMsg::Hello { protocol, .. } => {
            write_msg(&mut writer, &ServerMsg::Error {
                msg: format!("Nekompatibilní verze protokolu: očekáváno {PROTOCOL_VERSION}, dostáno {protocol}"),
            }).await?;
            return Ok(());
        }
        _ => {
            write_msg(&mut writer, &ServerMsg::Error { msg: "Očekáváno Hello".into() }).await?;
            return Ok(());
        }
    };

    // Odpověd Hello
    write_msg(&mut writer, &ServerMsg::Hello {
        protocol:    PROTOCOL_VERSION,
        server_name: "RTS Dedicated Server".into(),
        player_id:   id,
    }).await?;

    log::info!("Klient #{id} ({player_name}) – handshake OK");

    let handle = ClientHandle { id, name: player_name, tx };

    // Odeslat seznam lobby
    {
        let m = mgr.lock().await;
        let _ = handle.tx.send(ServerMsg::LobbyList { lobbies: m.lobby_list() });
    }

    // Spawn writer task
    let mut write_half = writer;
    let write_handle = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = write_msg(&mut write_half, &msg).await {
                log::debug!("write chyba: {e}");
                break;
            }
        }
    });

    // Čtení zpráv od klienta
    let result = recv_loop(id, &mut reader, &handle, Arc::clone(&mgr)).await;

    // Cleanup – odeber klienta ze všech lobby
    {
        let mut m = mgr.lock().await;
        m.leave_lobby(id);
    }

    write_handle.abort();
    result
}

async fn recv_loop<R: tokio::io::AsyncRead + Unpin>(
    id:     u64,
    reader: &mut R,
    handle: &ClientHandle,
    mgr:    Arc<Mutex<LobbyManager>>,
) -> std::io::Result<()> {
    loop {
        let msg: ClientMsg = read_msg(reader).await?;

        match msg {
            ClientMsg::Hello { .. } => {
                let _ = handle.tx.send(ServerMsg::Error { msg: "Duplikátní Hello".into() });
            }

            ClientMsg::RequestLobbyList => {
                let m = mgr.lock().await;
                let _ = handle.tx.send(ServerMsg::LobbyList { lobbies: m.lobby_list() });
            }

            ClientMsg::CreateLobby { name, max_players, map_id } => {
                let mut m = mgr.lock().await;
                let lobby = m.create_lobby(handle, name, max_players.min(8).max(1), map_id);
                let _ = handle.tx.send(ServerMsg::LobbyJoined { lobby });
            }

            ClientMsg::JoinLobby { id: lobby_id } => {
                let mut m = mgr.lock().await;
                match m.join_lobby(lobby_id, handle) {
                    Ok(lobby) => {
                        let _ = handle.tx.send(ServerMsg::LobbyJoined { lobby: lobby.clone() });
                        // Notifikuj ostatní
                        for p in m.lobbies.get(&lobby_id)
                            .map(|l| l.players.values().map(|p| p.handle.clone()).collect::<Vec<_>>())
                            .unwrap_or_default()
                        {
                            if p.id != handle.id {
                                let _ = p.tx.send(ServerMsg::LobbyUpdated { lobby: lobby.clone() });
                            }
                        }
                    }
                    Err(e) => { let _ = handle.tx.send(ServerMsg::Error { msg: e }); }
                }
            }

            ClientMsg::LeaveLobby => {
                let mut m = mgr.lock().await;
                m.leave_lobby(handle.id);
                let _ = handle.tx.send(ServerMsg::LobbyLeft);
            }

            ClientMsg::SetReady { ready } => {
                let mut m = mgr.lock().await;
                m.set_ready(handle.id, ready);
            }

            ClientMsg::StartGame => {
                let (scripts_dir, assets_dir) = {
                    let m = mgr.lock().await;
                    (m.scripts_dir.clone(), m.assets_dir.clone())
                };
                let mut m = mgr.lock().await;
                if let Err(e) = m.start_game(handle.id, scripts_dir, assets_dir) {
                    let _ = handle.tx.send(ServerMsg::Error { msg: e });
                }
            }

            ClientMsg::ChatMsg { text } => {
                let m = mgr.lock().await;
                m.broadcast_all(ServerMsg::ChatMsg {
                    from: handle.name.clone(),
                    text,
                });
            }

            ClientMsg::PlayerInput { tick, actions } => {
                let m = mgr.lock().await;
                m.deliver_input(handle.id, tick, actions);
            }
        }
    }
}

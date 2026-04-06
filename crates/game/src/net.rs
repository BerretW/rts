//! Síťový klient – běží na pozadí ve vlastním OS threadu.
//!
//! Komunikace s herním vláknem přes std::sync::mpsc.

use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};

use net::{ClientMsg, ServerMsg, DEFAULT_PORT, PROTOCOL_VERSION};

// ── Veřejné API ───────────────────────────────────────────────────────────────

pub struct NetClient {
    /// Zprávy přijaté od serveru (herní vlákno čte).
    pub recv: std_mpsc::Receiver<ServerMsg>,
    /// Stav připojení.
    pub state: Arc<Mutex<ConnState>>,
    /// Sender pro posílání zpráv na server.
    send_tx: std_mpsc::SyncSender<ClientMsg>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ConnState {
    Connecting,
    Connected,
    Disconnected(String),
}

impl NetClient {
    /// Zahájí připojení k `host:port` s daným jménem hráče.
    /// Vrátí `NetClient` okamžitě (připojení probíhá na pozadí).
    pub fn connect(host: &str, port: u16, player_name: String) -> Self {
        let (recv_tx, recv_rx)   = std_mpsc::channel::<ServerMsg>();
        let (send_tx, send_rx)   = std_mpsc::sync_channel::<ClientMsg>(64);
        let state                = Arc::new(Mutex::new(ConnState::Connecting));
        let state2               = Arc::clone(&state);

        let addr = format!("{host}:{port}");

        std::thread::spawn(move || {
            // Spustíme jednoduchý tokio runtime v tomto threadu
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all().build()
            {
                Ok(r) => r,
                Err(e) => {
                    *state2.lock().unwrap() = ConnState::Disconnected(e.to_string());
                    return;
                }
            };
            rt.block_on(async move {
                if let Err(e) = async_loop(addr, player_name, recv_tx, send_rx, Arc::clone(&state2)).await {
                    *state2.lock().unwrap() = ConnState::Disconnected(e.to_string());
                }
            });
        });

        NetClient { recv: recv_rx, send_tx, state }
    }

    /// Pošle zprávu serveru (neblokující).
    pub fn send(&self, msg: ClientMsg) {
        let _ = self.send_tx.try_send(msg);
    }

    /// Vrátí všechny dosud přijaté zprávy (neblokující).
    pub fn drain(&self) -> Vec<ServerMsg> {
        let mut out = Vec::new();
        while let Ok(msg) = self.recv.try_recv() {
            out.push(msg);
        }
        out
    }

    pub fn conn_state(&self) -> ConnState {
        self.state.lock().unwrap().clone()
    }
}

// ── Async smyčka ─────────────────────────────────────────────────────────────

async fn async_loop(
    addr:        String,
    player_name: String,
    recv_tx:     std_mpsc::Sender<ServerMsg>,
    send_rx:     std_mpsc::Receiver<ClientMsg>,
    state:       Arc<Mutex<ConnState>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::net::TcpStream;
    use net::{read_msg, write_msg};

    log::info!("Připojuji se k {addr}…");
    let stream = TcpStream::connect(&addr).await?;
    stream.set_nodelay(true)?;
    let (mut reader, mut writer) = tokio::io::split(stream);

    // Odešli Hello
    write_msg(&mut writer, &ClientMsg::Hello {
        protocol:    PROTOCOL_VERSION,
        player_name: player_name.clone(),
    }).await?;

    // Přijmi Hello od serveru
    let hello: ServerMsg = read_msg(&mut reader).await?;
    match &hello {
        ServerMsg::Hello { .. } => {}
        ServerMsg::Error { msg } => return Err(msg.clone().into()),
        _ => return Err("Neočekávaná odpověď na Hello".into()),
    }
    // Předej Hello hernímu vláknu (obsahuje player_id)
    let _ = recv_tx.send(hello);

    *state.lock().unwrap() = ConnState::Connected;
    log::info!("Připojeno k {addr} jako \"{player_name}\"");

    // Spawn writer task
    let (write_tx, mut write_rx) = tokio::sync::mpsc::unbounded_channel::<ClientMsg>();
    let mut write_half = writer;
    tokio::spawn(async move {
        while let Some(msg) = write_rx.recv().await {
            if let Err(e) = write_msg(&mut write_half, &msg).await {
                log::debug!("write: {e}");
                break;
            }
        }
    });

    // Přeposílej zprávy z std_mpsc → tokio::mpsc (v pozadí)
    let write_tx2 = write_tx.clone();
    std::thread::spawn(move || {
        while let Ok(msg) = send_rx.recv() {
            if write_tx2.send(msg).is_err() { break; }
        }
    });

    // Čti zprávy ze serveru a předávej do recv_tx
    loop {
        match read_msg::<_, ServerMsg>(&mut reader).await {
            Ok(msg) => {
                if recv_tx.send(msg).is_err() { break; }
            }
            Err(e) => {
                *state.lock().unwrap() = ConnState::Disconnected(e.to_string());
                break;
            }
        }
    }

    Ok(())
}

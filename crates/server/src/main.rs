//! RTS Dedicated Server
//!
//! Spuštění: `rts-server [--port 7777] [--scripts ./scripts] [--assets ./assets]`

mod lobby;
mod game_session;
mod client_conn;
mod world;      // server-side ECS komponenty
mod systems;    // server-side herní systémy
mod scripting;  // server-side Lua runtime

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use lobby::LobbyManager;

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let port          = parse_arg(&args, "--port",      "7777");
    let resources_dir = parse_arg(&args, "--resources", "resources");
    let assets_dir    = parse_arg(&args, "--assets",    "assets");

    let addr: SocketAddr = format!("0.0.0.0:{port}").parse().expect("neplatná adresa");
    let listener = TcpListener::bind(addr).await.expect("nelze bindovat port");
    log::info!("RTS Server naslouchá na {addr}");
    log::info!("Resources: {resources_dir}  |  Assets: {assets_dir}");

    let lobby_mgr = Arc::new(Mutex::new(LobbyManager::new(
        resources_dir.into(),
        assets_dir.into(),
    )));

    let mut next_client_id: u64 = 1;

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let id = next_client_id;
                next_client_id += 1;
                log::info!("Klient #{id} připojen z {addr}");

                let mgr = Arc::clone(&lobby_mgr);
                tokio::spawn(async move {
                    if let Err(e) = client_conn::run(id, stream, mgr).await {
                        log::info!("Klient #{id} odpojen: {e}");
                    }
                });
            }
            Err(e) => log::error!("accept() selhalo: {e}"),
        }
    }
}

fn parse_arg(args: &[String], flag: &str, default: &str) -> String {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].clone())
        .unwrap_or_else(|| default.to_owned())
}

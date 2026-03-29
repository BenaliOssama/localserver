// src/main.rs

mod config;
mod epoll;
mod handler;
mod request;
mod response;
mod server;
mod tui;
mod utils;
mod cgi;

use std::sync::mpsc;
use std::thread;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.conf".to_string());

    let config = config::Config::from_file(&config_path)?;
    //("Loaded {} server(s) from '{}'", config.servers.len(), config_path);

    // Create the TUI channel
    let (tx, rx) = mpsc::channel::<tui::ServerEvent>();

    // Group servers by host:port
    let mut groups: std::collections::HashMap<String, Vec<config::ServerConfig>> =
        std::collections::HashMap::new();
    for srv in config.servers {
        groups.entry(srv.addr()).or_default().push(srv);
    }

    // Spawn one thread per port group
    for (addr, servers) in groups {
        let tx = tx.clone();
        thread::spawn(move || {
            let srv = server::Server::new(servers, Some(tx));
            if let Err(e) = srv.run() {
                //e//("Server on {} failed: {}", addr, e);
            }
        });
    }

    // Run TUI on main thread — blocks until 'q'
    tui::run(rx);

    Ok(())
}
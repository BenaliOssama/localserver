mod cgi;
mod config;
mod epoll;
mod handler;
mod request;
mod response;
mod server;
mod utils;

use std::thread;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read config path from args, default to "config.conf"
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.conf".to_string());

    let config = config::Config::from_file(&config_path)?;

    println!(
        "Loaded {} server(s) from '{}'",
        config.servers.len(),
        config_path
    );

    // Group servers by host:port
    let mut groups: std::collections::HashMap<String, Vec<config::ServerConfig>> =
        std::collections::HashMap::new();

    for server in config.servers {
        groups
            .entry(server.addr())
            .or_insert_with(Vec::new)
            .push(server);
    }
    // Spawn one thread per unique host:port
    let handles: Vec<_> = groups
        .into_iter()
        .map(|(addr, servers)| {
            thread::spawn(move || {
                println!("Starting listener on http://{}", addr);
                let srv = server::Server::new(servers);
                if let Err(e) = srv.run() {
                    eprintln!("Server on {} failed: {}", addr, e);
                }
            })
        })
        .collect();

    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
}

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

    // Spin up one thread per server block
    let handles: Vec<_> = config
        .servers
        .into_iter()
        .map(|server_config| {
            thread::spawn(move || {
                let addr = server_config.addr();
                let srv = server::Server::new(server_config);
                println!("Starting server on http://{}", addr);
                if let Err(e) = srv.run() {
                    eprintln!("Server on {} failed: {}", addr, e);
                }
            })
        })
        .collect();

    // Wait for all servers — runs forever unless one crashes
    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
}

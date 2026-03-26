// src/server.rs

use libc::epoll_event;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::io::AsRawFd;
use std::time::Instant;

use crate::config::ServerConfig;
use crate::epoll::{Epoll, MAX_EVENTS, set_nonblocking};
use crate::handler;
use crate::request::Request;
use crate::response::{Response, StatusCode};

pub struct Server {
    configs: Vec<ServerConfig>,
}

impl Server {
    pub fn new(configs: Vec<ServerConfig>) -> Server {
        Server { configs }
    }
    // Find the right config for this request based on Host header
    fn match_config(&self, req: &Request) -> &ServerConfig {
        let host = req
            .headers
            .get("host")
            .map(|h| h.split(':').next().unwrap_or(""))
            .unwrap_or("");

        // Try to find a matching server_name
        self.configs
            .iter()
            .find(|c| c.server_name.as_deref() == Some(host))
            // Fall back to first config if no match
            .unwrap_or(&self.configs[0])
    }
    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Use first config's addr to bind
        let addr = self.configs[0].addr();
        let listener = TcpListener::bind(&addr)?;
        set_nonblocking(listener.as_raw_fd())?;
        println!("Server listening on http://{}", addr);

        let epoll = Epoll::new()?;
        epoll.add(listener.as_raw_fd())?;

        let mut buffers: HashMap<i32, Vec<u8>> = HashMap::new();
        let mut connect_times: HashMap<i32, Instant> = HashMap::new();

        let mut events = vec![epoll_event { events: 0, u64: 0 }; MAX_EVENTS];

        const TIMEOUT_SECS: u64 = 30;

        loop {
            let ready = epoll.wait(&mut events, 1000)?;

            // ── Timeout check ─────────────────────────────────────────────
            let now = Instant::now();
            let timed_out: Vec<i32> = connect_times
                .iter()
                .filter(|(_, t)| now.duration_since(**t).as_secs() > TIMEOUT_SECS)
                .map(|(fd, _)| *fd)
                .collect();

            for fd in timed_out {
                eprintln!("Connection {} timed out", fd);
                let _ = epoll.remove(fd);
                buffers.remove(&fd);
                connect_times.remove(&fd);
                unsafe { libc::close(fd) };
            }

            // ── Handle ready events ───────────────────────────────────────
            for i in 0..ready {
                let fd = events[i].u64 as i32;

                if fd == listener.as_raw_fd() {
                    self.accept_connections(&listener, &epoll, &mut buffers, &mut connect_times)?;
                } else {
                    self.handle_client(fd, &epoll, &mut buffers);
                    connect_times.remove(&fd);
                }
            }
        }
    }

    fn accept_connections(
        &self,
        listener: &TcpListener,
        epoll: &Epoll,
        buffers: &mut HashMap<i32, Vec<u8>>,
        connect_times: &mut HashMap<i32, Instant>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            match listener.accept() {
                Ok((stream, addr)) => {
                    println!("[{}] New connection: {}", self.configs[0].addr(), addr);
                    let fd = stream.as_raw_fd();
                    set_nonblocking(fd)?;
                    epoll.add(fd)?;
                    buffers.insert(fd, Vec::new());
                    connect_times.insert(fd, Instant::now());
                    std::mem::forget(stream);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    eprintln!("Accept error: {}", e);
                    break;
                }
            }
        }
        Ok(())
    }

    fn handle_client(&self, fd: i32, epoll: &Epoll, buffers: &mut HashMap<i32, Vec<u8>>) {
        let mut buf = [0u8; 4096];
        let mut stream = unsafe {
            use std::os::unix::io::FromRawFd;
            std::net::TcpStream::from_raw_fd(fd)
        };

        // ── Drain the socket ──────────────────────────────────────────────
        loop {
            match stream.read(&mut buf) {
                Ok(0) => {
                    println!("Client {} disconnected", fd);
                    let _ = epoll.remove(fd);
                    buffers.remove(&fd);
                    std::mem::forget(stream);
                    return;
                }
                Ok(n) => {
                    if let Some(buffer) = buffers.get_mut(&fd) {
                        buffer.extend_from_slice(&buf[..n]);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    eprintln!("Read error on fd {}: {}", fd, e);
                    let _ = epoll.remove(fd);
                    buffers.remove(&fd);
                    std::mem::forget(stream);
                    return;
                }
            }
        }

        // ── Check body size limit ─────────────────────────────────────────
        if let Some(data) = buffers.get(&fd) {
            if data.len() > self.configs[0].client_max_body_size {
                eprintln!("Client {} exceeded max body size", fd);
                Response::error(StatusCode::ContentTooLarge).send(&mut stream);
                let _ = epoll.remove(fd);
                buffers.remove(&fd);
                std::mem::forget(stream);
                return;
            }
        }

        // ── Parse and handle ──────────────────────────────────────────────
        if let Some(data) = buffers.get(&fd) {
            match Request::parse(data) {
                Some(req) => {
                    let config = self.match_config(&req);
                    println!("[{:?}] {:?} {}", config, req.method, req.path);
                    handler::handle(&mut req.clone(), &mut stream, config);
                }
                None => {
                    Response::error(StatusCode::BadRequest).send(&mut stream);
                }
            }
        }

        let _ = epoll.remove(fd);
        buffers.remove(&fd);
        std::mem::forget(stream);
    }
}

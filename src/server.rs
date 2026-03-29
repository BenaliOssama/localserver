// src/server.rs

use libc::epoll_event;
use std::collections::HashMap;
use std::io::Read;
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

        let mut read_buffers: HashMap<i32, Vec<u8>> = HashMap::new();
        let mut write_buffers: HashMap<i32, Vec<u8>> = HashMap::new();
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
                read_buffers.remove(&fd);
                write_buffers.remove(&fd);
                connect_times.remove(&fd);
                unsafe { libc::close(fd) };
            }

            // ── Handle ready events ───────────────────────────────────────
            for i in 0..ready {
                let fd = events[i].u64 as i32;
                let flags = events[i].events;

                if fd == listener.as_raw_fd() {
                    self.accept_connections(
                        &listener,
                        &epoll,
                        &mut read_buffers,
                        &mut connect_times,
                    )?;
                } else if flags & libc::EPOLLOUT as u32 != 0 {
                    // Socket ready to write
                    self.handle_write(fd, &epoll, &mut write_buffers);
                    connect_times.remove(&fd);
                } else {
                    // Socket ready to read
                    self.handle_read(fd, &epoll, &mut read_buffers, &mut write_buffers);
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
    fn handle_read(
        &self,
        fd: i32,
        epoll: &Epoll,
        read_buffers: &mut HashMap<i32, Vec<u8>>,
        write_buffers: &mut HashMap<i32, Vec<u8>>,
    ) {
        let mut buf = [0u8; 4096];
        let mut stream = unsafe {
            use std::os::unix::io::FromRawFd;
            std::net::TcpStream::from_raw_fd(fd)
        };

        // ── Drain the socket ──────────────────────────────────────────────
        loop {
            match stream.read(&mut buf) {
                Ok(0) => {
                    // Client disconnected
                    let _ = epoll.remove(fd);
                    read_buffers.remove(&fd);
                    write_buffers.remove(&fd);
                    std::mem::forget(stream);
                    return;
                }
                Ok(n) => {
                    if let Some(buffer) = read_buffers.get_mut(&fd) {
                        buffer.extend_from_slice(&buf[..n]);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    eprintln!("Read error on fd {}: {}", fd, e);
                    let _ = epoll.remove(fd);
                    read_buffers.remove(&fd);
                    write_buffers.remove(&fd);
                    std::mem::forget(stream);
                    unsafe { libc::close(fd) };
                    return;
                }
            }
        }

        // ── Check body size limit ─────────────────────────────────────────
        if let Some(data) = read_buffers.get(&fd) {
            let config = match Request::parse(data) {
                Some(ref req) => self.match_config(req),
                None => &self.configs[0],
            };
            if data.len() > config.client_max_body_size {
                let response = Response::error(StatusCode::ContentTooLarge);
                write_buffers.insert(fd, self.serialize_response(&response));
                let _ = epoll.watch_write(fd);
                std::mem::forget(stream);
                return;
            }
        }

        // ── Parse and build response ──────────────────────────────────────
        if let Some(data) = read_buffers.get(&fd) {
            let response = match Request::parse(data) {
                Some(mut req) => {
                    // Extract query string
                    let query_string = if let Some((path, query)) = req.path.clone().split_once('?')
                    {
                        req.path = path.to_string();
                        query.to_string()
                    } else {
                        String::new()
                    };
                    req.headers
                        .insert("x-query-string".to_string(), query_string);

                    let config = self.match_config(&req);
                    handler::build_response(req, config) // ← returns Response, doesn't send
                }
                None => Response::error(StatusCode::BadRequest),
            };

            // Store response bytes in write buffer
            write_buffers.insert(fd, self.serialize_response(&response));

            // Tell epoll we want to write
            let _ = epoll.watch_write(fd);
        }

        read_buffers.remove(&fd);
        std::mem::forget(stream);
    }

    fn handle_write(&self, fd: i32, epoll: &Epoll, write_buffers: &mut HashMap<i32, Vec<u8>>) {
        use std::io::Write;

        let mut stream = unsafe {
            use std::os::unix::io::FromRawFd;
            std::net::TcpStream::from_raw_fd(fd)
        };

        if let Some(data) = write_buffers.get_mut(&fd) {
            loop {
                if data.is_empty() {
                    break;
                }
                match stream.write(data) {
                    Ok(0) => {
                        let _ = epoll.remove(fd);
                        write_buffers.remove(&fd);
                        std::mem::forget(stream);
                        unsafe { libc::close(fd) };
                        return;
                    }
                    Ok(n) => {
                        data.drain(..n);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // Can't write yet — stay in EPOLLOUT mode
                        std::mem::forget(stream);
                        return;
                    }
                    Err(e) => {
                        eprintln!("Write error on fd {}: {}", fd, e);
                        let _ = epoll.remove(fd);
                        write_buffers.remove(&fd);
                        std::mem::forget(stream);
                        unsafe { libc::close(fd) };
                        return;
                    }
                }
            }
        }

        // Write complete — clean up
        let _ = epoll.remove(fd);
        write_buffers.remove(&fd);
        std::mem::forget(stream);
        unsafe {libc::close(fd)};
    }
    fn serialize_response(&self, response: &Response) -> Vec<u8> {
        let status = if response.redirect_location.is_some() {
            "301 Moved Permanently"
        } else {
            response.status.as_str()
        };

        let location_header = response
            .redirect_location
            .as_ref()
            .map(|l| format!("Location: {}\r\n", l))
            .unwrap_or_default();

        let cookie_headers: String = response
            .cookies
            .iter()
            .map(|c| format!("Set-Cookie: {}\r\n", c))
            .collect();

        let header = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n{}{}\r\n",
            status,
            response.content_type,
            response.body.len(),
            location_header,
            cookie_headers,
        );

        let mut bytes = header.into_bytes();
        bytes.extend_from_slice(&response.body);
        bytes
    }
}

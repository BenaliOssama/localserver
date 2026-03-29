use mio::{Events, Interest, Poll, Token};
use mio::net::TcpListener;

use std::collections::HashMap;
use std::net::SocketAddr;

fn main() -> std::io::Result<()> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(128);

    let ports = [8000, 9000, 7000];

    let mut listeners: HashMap<Token, TcpListener> = HashMap::new();

    // create listeners
    for (i, port) in ports.iter().enumerate() {
        let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();

        let mut listener = TcpListener::bind(addr)?;
        let token = Token(i);

        poll.registry()
            .register(&mut listener, token, Interest::READABLE)?;

        listeners.insert(token, listener);

        ////("Listening on {}", addr);
    }

    loop {
        poll.poll(&mut events, None)?;

        for event in events.iter() {
            let token = event.token();

            if let Some(listener) = listeners.get_mut(&token) {
                loop {
                    match listener.accept() {
                        Ok((stream, client_addr)) => {
                            let local_addr = stream.local_addr()?;

                            //("Client: {}", client_addr);
                            //("Server IP+Port called: {}", local_addr);
                            //("---");
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            break;
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }
}
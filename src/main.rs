use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

fn handle_connection(mut stream: TcpStream) {
    let mut buffer = [0; 1024];
    stream.read(&mut buffer).unwrap();

    // Print what the browser sent us
    println!("--- Request ---\n{}", String::from_utf8_lossy(&buffer));

    // A valid HTTP response
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, World!";

    stream.write_all(response.as_bytes()).unwrap();
}

it fn main() {
    let listener = TcpListener::bind("127.0.0.1:8081").unwrap();
    println!("Server listening on http://localhost:8080");

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        handle_connection(stream);
    }
}

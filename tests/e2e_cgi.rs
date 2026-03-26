// tests/e2e_server.rs

mod common;
use common::*;
use std::thread;
use std::time::Duration;

#[test]
fn e2e_get_returns_200_or_404() {
    let port = start_server();
    let response = send_request(
        port,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(status_line(&response).contains("200") || status_line(&response).contains("404"));
}

#[test]
fn e2e_get_missing_returns_404() {
    let port = start_server();
    let response = send_request(
        port,
        "GET /this-does-not-exist HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(status_line(&response).contains("404 Not Found"));
}

#[test]
fn e2e_response_is_valid_http() {
    let port = start_server();
    let response = send_request(
        port,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    let raw = String::from_utf8_lossy(&response);
    assert!(raw.starts_with("HTTP/1.1"));
    assert!(raw.contains("\r\n\r\n"));
    assert!(header(&response, "content-length").is_some());
}

#[test]
fn e2e_content_length_matches_body() {
    let port = start_server();
    let response = send_request(
        port,
        "GET /missing.html HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    let declared_len: usize = header(&response, "content-length")
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(declared_len, body(&response).len());
}

#[test]
fn e2e_post_upload_and_retrieve() {
    let port = start_server();
    let content = "hello from e2e test";
    let upload = format!(
        "POST /uploads/e2e_test.txt HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        content.len(),
        content
    );
    let response = send_request(port, &upload);
    assert!(status_line(&response).contains("200 OK"));

    let retrieve = send_request(
        port,
        "GET /uploads/e2e_test.txt HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(status_line(&retrieve).contains("200 OK"));
    assert_eq!(body(&retrieve), content.as_bytes());
}

#[test]
fn e2e_post_empty_body_returns_400() {
    let port = start_server();
    let response = send_request(
        port,
        "POST /uploads/empty.txt HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    assert!(status_line(&response).contains("400 Bad Request"));
}

#[test]
fn e2e_delete_uploaded_file() {
    let port = start_server();
    send_request(
        port,
        &format!(
            "POST /uploads/to_delete.txt HTTP/1.1\r\nHost: localhost\r\nContent-Length: 4\r\nConnection: close\r\n\r\ndata"
        ),
    );
    let delete = send_request(
        port,
        "DELETE /uploads/to_delete.txt HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(status_line(&delete).contains("200 OK"));

    let confirm = send_request(
        port,
        "GET /uploads/to_delete.txt HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(status_line(&confirm).contains("404 Not Found"));
}

#[test]
fn e2e_delete_missing_file_returns_404() {
    let port = start_server();
    let response = send_request(
        port,
        "DELETE /ghost.txt HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(status_line(&response).contains("404 Not Found"));
}

#[test]
fn e2e_unknown_method_returns_405() {
    let port = start_server();
    let response = send_request(
        port,
        "PATCH / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(status_line(&response).contains("405 Method Not Allowed"));
}

#[test]
fn e2e_garbage_request_returns_400() {
    let port = start_server();
    let response = send_request(port, "GARBAGE\r\n\r\n");
    assert!(status_line(&response).contains("400 Bad Request"));
}

#[test]
fn e2e_empty_request_does_not_crash() {
    let port = start_server();
    let stream = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
    drop(stream);

    thread::sleep(Duration::from_millis(50));
    let response = send_request(
        port,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(!response.is_empty());
}

#[test]
fn e2e_handles_concurrent_connections() {
    let port = start_server();
    let handles: Vec<_> = (0..10).map(|i| {
        thread::spawn(move || {
            let response = send_request(port, &format!(
                "POST /uploads/concurrent_{}.txt HTTP/1.1\r\nHost: localhost\r\nContent-Length: 4\r\nConnection: close\r\n\r\ndata",
                i
            ));
            status_line(&response).contains("200 OK")
        })
    }).collect();

    let results: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    assert!(results.iter().all(|&ok| ok));
}

#[test]
fn e2e_server_survives_multiple_bad_requests() {
    let port = start_server();
    for _ in 0..5 {
        send_request(port, "GARBAGE DATA\r\n\r\n");
    }
    let response = send_request(
        port,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(!response.is_empty());
}

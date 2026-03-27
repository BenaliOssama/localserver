# localserver

A production-grade HTTP/1.1 server written from scratch in Rust. Built without any async runtimes or HTTP frameworks — just raw sockets, `epoll`, and the standard library.

```
cargo run config.conf
```

---

## Why this exists

Most web servers are black boxes. This one isn't. Every line — from the TCP handshake to the CGI fork — is written by hand and explained. The goal was to understand HTTP at the systems level, not just use it.

---

## Architecture

```
src/
├── main.rs         # Entry point — reads config, spawns one thread per port group
├── lib.rs          # Crate root — exposes modules for integration tests
├── server.rs       # epoll event loop — accepts connections, dispatches reads/writes
├── request.rs      # HTTP/1.1 parser — request line, headers, body, chunked encoding
├── response.rs     # HTTP response builder — status codes, headers, cookies
├── handler.rs      # Routing logic — matches locations, serves files, runs CGI
├── cgi.rs          # CGI runner — forks processes, pipes I/O, enforces timeouts
├── epoll.rs        # epoll wrapper — edge-triggered, non-blocking I/O
├── config/
│   ├── mod.rs          # Config structs + validation (duplicate detection)
│   ├── tokenizer.rs    # Lexer — raw text → tokens with line numbers
│   ├── parser.rs       # Recursive descent parser → ServerConfig structs
│   ├── errors.rs       # ParseError with line numbers for debugging
│   └── tests/
│       ├── mod.rs
│       ├── tokenizer_tests.rs
│       └── parser_tests.rs
└── utils/
    ├── mod.rs
    └── session.rs      # Session store — in-memory, cookie-backed, with expiry
```

The data flow is intentionally linear:

```
TCP connection
  └── epoll detects readable fd
        └── request.rs parses raw bytes → Request struct
              └── handler.rs matches location → builds Response
                    └── epoll detects writable fd → response bytes sent
```

---

## How it works

### I/O multiplexing with epoll

The server uses a single `epoll` instance per port to watch hundreds of connections simultaneously without threads. This is edge-triggered (`EPOLLET`) — the kernel fires once per new data arrival, so the server must drain the entire socket buffer before returning to `epoll_wait`.

```
epoll_wait() → fires on fd N
  if fd == listener  → accept all pending connections (loop until EAGAIN)
  if EPOLLIN         → read all available bytes (loop until EAGAIN)
  if EPOLLOUT        → write response bytes (loop until EAGAIN or done)
```

All reads and writes go through epoll. Nothing blocks.

### Request parsing

HTTP/1.1 requests arrive as raw bytes. The parser splits on `\r\n\r\n` to separate headers from body, then reads exactly `Content-Length` bytes for the body. Chunked transfer encoding (`Transfer-Encoding: chunked`) is decoded before the body reaches the handler.

### Config-driven routing

Routes are defined in `config.conf`. The handler uses longest-prefix matching — `/images/photo.jpg` matches `/images` before `/`. Each location specifies its root directory, allowed methods, and optional CGI.

### CGI

When a request matches a CGI location, the server forks a child process, sets environment variables (`REQUEST_METHOD`, `PATH_INFO`, `QUERY_STRING`, `CONTENT_LENGTH`), pipes the request body to stdin, and reads the response from stdout. Scripts that exceed 10 seconds are killed.

---

## Configuration

```nginx
server {
    host        127.0.0.1;
    port        8080;
    server_name mysite.com;
    client_max_body_size 20MB;

    error_page 404 ./error_pages/404.html;
    error_page 500 ./error_pages/500.html;
    error_page 403 ./error_pages/403.html;

    location / {
        root    ./www;
        index   index.html;
        methods GET;
        autoindex off;
    }

    location /uploads {
        root    ./www/uploads;
        methods GET POST DELETE;
    }

    location /files {
        root      ./files;
        autoindex on;
        methods   GET;
    }

    location /cgi {
        root    ./cgi-bin;
        methods GET POST;
        cgi     .py python3;
    }

    location /old-page {
        redirect /new-page;
    }
}
```

Multiple server blocks share the same port when `server_name` differs — the server dispatches by `Host` header.

**Validation at startup:**
- Duplicate `host:port:server_name` → rejected with a clear error
- Invalid port number, unknown directive, unclosed block → rejected with line number
- Missing required fields (`host`, `port`) → rejected

---

## Features

| Feature | Status |
|---|---|
| HTTP/1.1 GET / POST / DELETE | ✅ |
| Static file serving | ✅ |
| Directory listing (autoindex) | ✅ |
| File uploads | ✅ |
| Custom error pages | ✅ |
| Chunked transfer encoding | ✅ |
| HTTP redirects (301) | ✅ |
| CGI (Python) | ✅ |
| Cookies and sessions | ✅ |
| Multiple servers on multiple ports | ✅ |
| Virtual hosting (server_name) | ✅ |
| Client body size limit | ✅ |
| Connection timeouts (30s) | ✅ |
| Edge-triggered epoll | ✅ |
| Non-blocking I/O throughout | ✅ |
| Config file validation | ✅ |

---

## Getting started

**Requirements:** Rust 1.70+, Python 3 (for CGI tests)

```bash
# Build
cargo build --release

# Run
cargo run config.conf

# Run with release build
./target/release/localserver config.conf
```

Create the directory structure:

```bash
mkdir -p www www/uploads files cgi-bin error_pages
echo "<html><body><h1>Hello</h1></body></html>" > www/index.html
echo "<html><body><h1>404</h1></body></html>"   > error_pages/404.html
echo "<html><body><h1>500</h1></body></html>"   > error_pages/500.html
echo "<html><body><h1>403</h1></body></html>"   > error_pages/403.html
```

---

## Testing

### Unit tests

Test individual modules in isolation — no network, no disk where possible.

```bash
cargo test
```

| Module | Tests | What's covered |
|---|---|---|
| `request.rs` | 25 | Parsing, headers, cookies, chunked encoding, malformed input |
| `response.rs` | 12 | Wire format, status codes, cookie headers, binary bodies |
| `handler.rs` | 20 | File serving, method routing, 404/403/405, upload/delete |
| `epoll.rs` | 9 | fd lifecycle, non-blocking verification, drop behavior |
| `config/` | 41 | Tokenizer, parser, validation, error cases |
| `cgi.rs` | 14 | Output parsing, status forwarding, timeout, real scripts |

**107 tests total. All passing.**

### End-to-end tests

Spin up real servers on random ports, send real HTTP over TCP.

```bash
cargo test --test e2e_server
cargo test --test e2e_cgi
```

| Test file | Tests | What's covered |
|---|---|---|
| `e2e_server.rs` | 13 | Full HTTP cycle, upload/delete, concurrency, crash survival |
| `e2e_cgi.rs` | 13 | CGI execution, query strings, POST bodies, timeouts, crashes |

### Makefile tests

```bash
make test-all       # Upload → retrieve → delete → verify
make test-cgi       # GET, POST, query string via real CGI script
make test-login     # Login → whoami → logout → 403
```

### Stress tests

```bash
# Full stress test — 7 phases, ~5 minutes
./scripts/stress_test.sh

# Audit checklist — covers every evaluation requirement
./scripts/audit_test.sh
```

**Stress test phases:**

| Phase | What it does |
|---|---|
| 1 — Health check | GET/404/405 baseline |
| 2 — Concurrent GETs | 10 / 50 / 100 simultaneous requests |
| 3 — Concurrent POSTs | 100 uploads + disk verification + round-trip integrity |
| 4 — DELETE + mixed | 50 concurrent deletes + 150 mixed method requests |
| 5 — Bad input | Garbage, empty connections, partial requests, oversized bodies |
| 6 — Siege | 10 → 25 → 50 → 100 → 255 concurrent users, 60s sustained |
| 7 — Cleanup | Removes all test artifacts |

---

## Benchmarks

Tested on a mid-range laptop (Intel i5, 8GB RAM):

```
siege -b -t 30s -c 255 http://127.0.0.1:8080/

Transactions:          ~180,000
Availability:            100.00%
Transaction rate:        ~6,000 req/s
Response time:            0.04s avg
Failed transactions:           0
```

Availability stays above 99.5% at all concurrency levels tested (10 → 255 concurrent users).

---

## Answering the audit questions

**How does an HTTP server work?**
A server binds a TCP socket to a port and calls `accept()` in a loop. For each connection, it reads bytes, parses them as an HTTP request, builds a response, and writes it back. The connection is then closed (HTTP/1.0) or reused (HTTP/1.1 keep-alive).

**Which function for I/O multiplexing and how does it work?**
`epoll` (Linux). You create an epoll instance with `epoll_create1()`, register file descriptors with `epoll_ctl()`, and call `epoll_wait()` to block until one or more fds are ready. The kernel maintains the watch list in the kernel space — unlike `select`, it doesn't scan all fds on every call, making it O(1) per event rather than O(n).

**Is there only one epoll for reads and writes?**
Yes — `src/server.rs` creates one `Epoll` instance per port. The same instance watches both the listening socket and all client sockets. When a client socket becomes readable, `EPOLLIN` fires. When it becomes writable, `EPOLLOUT` fires. We switch between them with `EPOLL_CTL_MOD`.

**Why only one epoll?**
Multiple epoll instances would require coordinating which instance owns which fd, introducing complexity and potential race conditions. One instance per port gives a complete, consistent view of all connections on that port.

**Is there only one read/write per client per epoll event?**
In edge-triggered mode, each `epoll_wait` event means new data has arrived since the last check. We drain the entire socket buffer in a loop (until `EAGAIN`), but this counts as one logical read per epoll event — it's the only correct approach with `EPOLLET`.

**Are return values checked?**
Yes. Every `read()`, `write()`, `epoll_ctl()`, and `epoll_wait()` call checks its return value. Errors on client sockets remove the client from epoll and close the fd. Errors on the epoll fd itself propagate up to `main` and exit cleanly.

**Is writing always done through epoll?**
Yes. When a response is ready, it's stored in a write buffer. We call `epoll.watch_write(fd)` to register `EPOLLOUT`. When `epoll_wait` fires with `EPOLLOUT`, we write from the buffer. We never call `write()` outside of an epoll event.

---

## Project structure at a glance

```
localserver/
├── src/                    # Server source
├── tests/
│   ├── common/mod.rs       # Shared test helpers and server factories
│   ├── e2e_server.rs       # End-to-end server tests
│   └── e2e_cgi.rs          # End-to-end CGI tests
├── scripts/
│   ├── stress_test.sh      # 7-phase stress test
│   └── audit_test.sh       # Audit checklist (covers evaluation sheet)
├── cgi-bin/
│   └── hello.py            # Example CGI script
├── www/                    # Default document root
├── error_pages/            # Custom error pages
└── config.conf             # Server configuration
```

---

## What's not implemented

- HTTPS / TLS
- HTTP/2
- Keep-alive connection reuse
- Gzip compression
- Virtual file system
- Authentication beyond the demo session system

These are outside the project scope but the architecture supports adding them — the request/response pipeline is cleanly separated from the transport layer.
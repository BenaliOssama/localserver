// src/tui.rs

use std::collections::VecDeque;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
};

struct TuiState {
    servers: std::collections::HashMap<u16, ServerStats>,
    log:     VecDeque<LogEntry>,
    errors:  VecDeque<String>,  // ← add this
}




#[derive(Debug, Clone)]
pub enum LogLevel { Info, Warn, Error }
// ── Events the server sends to the TUI ───────────────────────────────────────
#[derive(Debug, Clone)]
pub enum ServerEvent {
    Request {
        port: u16,
        method: String,
        path: String,
        status: u16,
        duration_ms: u64,
    },
    ConnectionOpened {
        port: u16,
    },
    ConnectionClosed {
        port: u16,
    },
    ServerStarted {
        port: u16,
        addr: String,
    },
    Log {
        level: LogLevel,
        message: String,
    }, // ← add this
}

// ── Per-server stats ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ServerStats {
    addr: String,
    port: u16,
    total_requests: u64,
    total_errors: u64,
    active_connections: i64,
    requests_this_sec: u64,
    req_per_sec: f64,
    last_sec_reset: Instant,
}

impl ServerStats {
    fn new(port: u16, addr: String) -> Self {
        ServerStats {
            addr,
            port,
            total_requests: 0,
            total_errors: 0,
            active_connections: 0,
            requests_this_sec: 0,
            req_per_sec: 0.0,
            last_sec_reset: Instant::now(),
        }
    }

    fn tick(&mut self) {
        let elapsed = self.last_sec_reset.elapsed();
        if elapsed >= Duration::from_secs(1) {
            self.req_per_sec = self.requests_this_sec as f64 / elapsed.as_secs_f64();
            self.requests_this_sec = 0;
            self.last_sec_reset = Instant::now();
        }
    }
}

// ── Log entry ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct LogEntry {
    time: String,
    port: u16,
    method: String,
    path: String,
    status: u16,
    duration_ms: u64,
}

// ── TUI state ─────────────────────────────────────────────────────────────────

impl TuiState {
    fn new() -> Self {
        TuiState {
            servers: std::collections::HashMap::new(),
            log: VecDeque::with_capacity(200),
            errors: VecDeque::with_capacity(50),
        }
    }

    fn handle(&mut self, event: ServerEvent) {
        match event {
            ServerEvent::ServerStarted { port, addr } => {
                self.servers.insert(port, ServerStats::new(port, addr));
            }
            ServerEvent::ConnectionOpened { port } => {
                if let Some(s) = self.servers.get_mut(&port) {
                    s.active_connections += 1;
                }
            }
            ServerEvent::ConnectionClosed { port } => {
                if let Some(s) = self.servers.get_mut(&port) {
                    s.active_connections = (s.active_connections - 1).max(0);
                }
            }
            ServerEvent::Request {
                port,
                method,
                path,
                status,
                duration_ms,
            } => {
                if let Some(s) = self.servers.get_mut(&port) {
                    s.total_requests += 1;
                    s.requests_this_sec += 1;
                    if status >= 500 {
                        s.total_errors += 1;
                    }
                }

                // Keep last 200 log entries
                if self.log.len() >= 200 {
                    self.log.pop_back();
                }

                let now = chrono_time();
                self.log.push_front(LogEntry {
                    time: now,
                    port,
                    method,
                    path,
                    status,
                    duration_ms,
                });
            }
            ServerEvent::Log { level: _, message: _ } => {
                // TODO: Handle log event, e.g., add to a separate log or display
            }
        }
    }

    fn tick_all(&mut self) {
        for s in self.servers.values_mut() {
            s.tick();
        }
    }
}

fn chrono_time() -> String {
    // Simple time without chrono dependency
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

// ── Draw ──────────────────────────────────────────────────────────────────────

fn draw(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, state: &TuiState) {
    terminal
        .draw(|frame| {
            let size = frame.size();

            // Split screen: log left (60%), stats right (40%)
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(size);

            // ── Left: request log ─────────────────────────────────────────────
            let log_items: Vec<ListItem> = state
                .log
                .iter()
                .map(|entry| {
                    let status_color = match entry.status {
                        200..=299 => Color::Green,
                        300..=399 => Color::Cyan,
                        400..=499 => Color::Yellow,
                        _ => Color::Red,
                    };

                    let line = Line::from(vec![
                        Span::styled(
                            format!("{} ", entry.time),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(
                            format!(":{} ", entry.port),
                            Style::default().fg(Color::Cyan),
                        ),
                        Span::styled(
                            format!("{:<7}", entry.method),
                            Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<30} ", truncate(&entry.path, 30)),
                            Style::default().fg(Color::White),
                        ),
                        Span::styled(
                            format!("{} ", entry.status),
                            Style::default()
                                .fg(status_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{}ms", entry.duration_ms),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]);

                    ListItem::new(line)
                })
                .collect();

            let log_widget = List::new(log_items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Request Log ")
                    .title_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
            );

            frame.render_widget(log_widget, chunks[0]);

            // ── Right: server stats ───────────────────────────────────────────
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    std::iter::repeat(Constraint::Min(8))
                        .take(state.servers.len().max(1))
                        .collect::<Vec<_>>(),
                )
                .split(chunks[1]);

            let mut sorted_servers: Vec<&ServerStats> = state.servers.values().collect();
            sorted_servers.sort_by_key(|s| s.port);

            for (i, server) in sorted_servers.iter().enumerate() {
                if i >= right_chunks.len() {
                    break;
                }

                let block_area = right_chunks[i];

                // Split server block into sections
                let inner = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([
                        Constraint::Length(1), // addr
                        Constraint::Length(1), // req/s
                        Constraint::Length(1), // gauge
                        Constraint::Length(1), // connections
                        Constraint::Length(1), // totals
                    ])
                    .split(block_area);

                let block = Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" :{} ", server.port))
                    .title_style(
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    );

                frame.render_widget(block, block_area);

                // Addr
                frame.render_widget(
                    Paragraph::new(server.addr.as_str())
                        .style(Style::default().fg(Color::DarkGray)),
                    inner[0],
                );

                // req/s
                frame.render_widget(
                    Paragraph::new(format!("{:.0} req/s", server.req_per_sec)).style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    inner[1],
                );

                // Gauge — visual req/s bar capped at 1000 req/s
                let gauge_pct = (server.req_per_sec / 1000.0 * 100.0).min(100.0) as u16;
                let gauge = Gauge::default()
                    .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
                    .percent(gauge_pct);
                frame.render_widget(gauge, inner[2]);

                // Active connections
                frame.render_widget(
                    Paragraph::new(format!("connections: {}", server.active_connections))
                        .style(Style::default().fg(Color::Cyan)),
                    inner[3],
                );

                // Totals
                frame.render_widget(
                    Paragraph::new(format!(
                        "total: {}  errors: {}",
                        server.total_requests, server.total_errors
                    ))
                    .style(Style::default().fg(Color::White)),
                    inner[4],
                );
            }
        })
        .ok();
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn run(rx: mpsc::Receiver<ServerEvent>) {
    enable_raw_mode().unwrap();
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).unwrap();

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut state = TuiState::new();
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        // Drain all pending events
        while let Ok(event) = rx.try_recv() {
            state.handle(event);
        }
        state.tick_all();

        draw(&mut terminal, &state);

        // Check for 'q' to quit
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_default();

        if event::poll(timeout).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    // Restore terminal
    disable_raw_mode().unwrap();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).unwrap();
}

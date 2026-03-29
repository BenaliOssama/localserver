use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
// Global session store — one instance for the entire process
// fixed memory location
// lifetime = entire program execution ('static)
// It is initialized once and exists globally.
// Constraint: must be safe for concurrent access because it is shared.

//OnceLock<T>
// A container that:
// starts uninitialized
// allows exactly one successful initialization
// after initialization → provides immutable access to T
// Internal guarantee:
// initialization happens at most once, even under concurrency
// Operations:
// get() → returns reference if initialized
// set(value) or get_or_init(...) → initializes if not already

// Mutex<T>
// A synchronization primitive enforcing:
// mutual exclusion
// Constraint:
// at most one thread can access T mutably at a time
// Mechanism:
// lock() → blocks until exclusive access is acquired
// returns a guard that provides access to T
static STORE: OnceLock<Mutex<SessionStore>> = OnceLock::new();

pub fn store() -> &'static Mutex<SessionStore> {
    STORE.get_or_init(|| Mutex::new(SessionStore::new()))
}

#[derive(Debug)]
pub struct Session {
    pub data: HashMap<String, String>,
    pub created_at: Instant,
}

impl Session {
    fn new() -> Session {
        Session {
            data: HashMap::new(),
            created_at: Instant::now(),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > Duration::from_secs(3600) // 1 hour
    }
}

pub struct SessionStore {
    sessions: HashMap<String, Session>,
}
impl SessionStore {
    fn new() -> SessionStore {
        SessionStore {
            sessions: HashMap::new(),
        }
    }

    // Create a new session, return its ID
    pub fn create(&mut self) -> String {
        let id = generate_id();
        self.sessions.insert(id.clone(), Session::new());
        id
    }

    #[allow(dead_code)]
    // Get a session by ID — returns None if missing or expired
    pub fn get(&mut self, id: &str) -> Option<&Session> {
        // Evict if expired
        if let Some(s) = self.sessions.get(id) {
            if s.is_expired() {
                self.sessions.remove(id);
                return None;
            }
        }
        self.sessions.get(id)
    }

    // Get a mutable session by ID
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Session> {
        if let Some(s) = self.sessions.get(id) {
            if s.is_expired() {
                self.sessions.remove(id);
                return None;
            }
        }
        self.sessions.get_mut(id)
    }

    // Set a value in a session
    pub fn set(&mut self, id: &str, key: &str, value: &str) -> bool {
        if let Some(session) = self.get_mut(id) {
            session.data.insert(key.to_string(), value.to_string());
            true
        } else {
            false
        }
    }

    // Get a value from a session
    pub fn get_value(&mut self, id: &str, key: &str) -> Option<String> {
        self.get_mut(id)?.data.get(key).cloned()
    }

    // Delete a session (logout)
    pub fn destroy(&mut self, id: &str) {
        self.sessions.remove(id);
    }
    // Evict all expired sessions
    #[allow(dead_code)]
    pub fn cleanup(&mut self) {
        self.sessions.retain(|_, s| !s.is_expired());
    }
}
// Generate a random session ID using system entropy
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Combine timestamp + process ID for uniqueness
    // In production you'd use a proper CSPRNG
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();

    let pid = std::process::id();

    // Mix the bits around to reduce predictability
    let raw = (t as u64)
        .wrapping_mul(0x9e3779b97f4a7c15)
        .wrapping_add(pid as u64)
        .wrapping_mul(0x6c62272e07bb0142);

    format!("{:016x}", raw)
}

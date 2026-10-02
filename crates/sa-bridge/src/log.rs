//! Tiny logger: appends to `sa_bridge.log` next to gta_sa.exe and keeps a ring buffer
//! that the `logs` request serves.

use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const RING_CAP: usize = 200;
static RING: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

pub fn init() {
    let _ = std::fs::remove_file("sa_bridge.log");
    write(concat!("sa-bridge ", env!("CARGO_PKG_VERSION"), " loaded"));
}

pub fn write(msg: &str) {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let line = format!("[{secs}] {msg}");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open("sa_bridge.log") {
        let _ = writeln!(f, "{line}");
    }
    if let Ok(mut ring) = RING.lock() {
        if ring.len() == RING_CAP {
            ring.pop_front();
        }
        ring.push_back(line);
    }
}

pub fn snapshot() -> Vec<String> {
    RING.lock().map(|r| r.iter().cloned().collect()).unwrap_or_default()
}

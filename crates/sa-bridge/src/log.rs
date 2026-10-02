//! Tiny logger: appends to `sa_bridge.N.log` next to gta_sa.exe and keeps a ring buffer
//! that the `logs` request serves.

use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const RING_CAP: usize = 200;
static RING: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static FILE: OnceLock<String> = OnceLock::new();

/// `instance` picks the file name (`sa_bridge.N.log`) so parallel clients don't clash.
pub fn init(instance: Option<u8>) {
    let name = match instance {
        Some(n) => format!("sa_bridge.{n}.log"),
        None => "sa_bridge.noslot.log".to_string(),
    };
    let _ = std::fs::remove_file(&name);
    let _ = FILE.set(name);
    write(concat!("sa-bridge ", env!("CARGO_PKG_VERSION"), " loaded"));
}

pub fn write(msg: &str) {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let line = format!("[{secs}] {msg}");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(FILE.get().map(String::as_str).unwrap_or("sa_bridge.log")) {
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

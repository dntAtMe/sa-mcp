//! Time-series capture of player state, sampled on the game thread. One recording at a time.

use std::sync::mpsc::Sender;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use proto::Response;
use serde_json::json;

use crate::game;

struct Recording {
    start: Instant,
    duration: Duration,
    interval: Duration,
    next: Duration,
    columns: [Vec<f64>; COLUMNS.len()],
    reply: Sender<Response>,
}

const COLUMNS: [&str; 9] = ["t_ms", "x", "y", "z", "heading", "speed", "health", "in_vehicle", "frame"];

static ACTIVE: Mutex<Option<Recording>> = Mutex::new(None);

pub fn start(seconds: f32, hz: f32, reply: Sender<Response>) -> Result<(), String> {
    if !(0.0..=60.0).contains(&seconds) || !(0.5..=60.0).contains(&hz) {
        return Err("seconds must be 0-60 and hz 0.5-60".into());
    }
    let mut active = ACTIVE.lock().unwrap();
    if active.is_some() {
        return Err("a recording is already running".into());
    }
    *active = Some(Recording {
        start: Instant::now(),
        duration: Duration::from_secs_f32(seconds),
        interval: Duration::from_secs_f32(1.0 / hz),
        next: Duration::ZERO,
        columns: Default::default(),
        reply,
    });
    Ok(())
}

/// Game thread, once per frame.
pub unsafe fn tick() {
    let Ok(mut active) = ACTIVE.try_lock() else { return };
    let Some(rec) = active.as_mut() else { return };
    let elapsed = rec.start.elapsed();
    if elapsed >= rec.next {
        if let Some(s) = game::sample_player() {
            let row = [elapsed.as_secs_f64() * 1000.0, s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]];
            for (col, v) in rec.columns.iter_mut().zip(row) {
                col.push((v * 100.0).round() / 100.0);
            }
        }
        rec.next += rec.interval;
    }
    if elapsed >= rec.duration {
        let rec = active.take().unwrap();
        let mut data = serde_json::Map::new();
        data.insert("samples".into(), json!(rec.columns[0].len()));
        for (name, col) in COLUMNS.iter().zip(rec.columns) {
            data.insert((*name).into(), json!(col));
        }
        let _ = rec.reply.send(Response::ok(data.into()));
    }
}

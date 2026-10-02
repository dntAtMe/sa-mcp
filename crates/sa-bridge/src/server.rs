//! Line-delimited JSON server on 127.0.0.1:BRIDGE_PORT. One thread per connection.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

use proto::{Request, Response, BRIDGE_PORT};
use serde_json::json;

use crate::{addr, game, log, mem, Job, FRAMES_PUMPED, HOOKED, JOBS, SUPPORTED};

const GAME_THREAD_TIMEOUT: Duration = Duration::from_secs(3);

pub fn run() {
    let listener = match TcpListener::bind(("127.0.0.1", BRIDGE_PORT)) {
        Ok(l) => l,
        Err(e) => {
            log::write(&format!("bind 127.0.0.1:{BRIDGE_PORT} failed: {e}"));
            return;
        }
    };
    log::write(&format!("listening on 127.0.0.1:{BRIDGE_PORT}"));
    for stream in listener.incoming().flatten() {
        std::thread::spawn(move || {
            if let Err(e) = serve(stream) {
                log::write(&format!("connection ended: {e}"));
            }
        });
    }
}

fn serve(stream: TcpStream) -> std::io::Result<()> {
    let mut writer = stream.try_clone()?;
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Request>(&line) {
            Ok(req) => execute(req),
            Err(e) => Response::err(format!("bad request: {e}")),
        };
        let mut out = serde_json::to_string(&resp).unwrap_or_else(|_| "{\"ok\":false}".into());
        out.push('\n');
        writer.write_all(out.as_bytes())?;
    }
    Ok(())
}

fn execute(req: Request) -> Response {
    match req {
        Request::Status => status(),
        Request::Logs => Response::ok(json!({ "lines": log::snapshot() })),
        // Fault-safe (ReadProcessMemory), so serve it even while the game thread is busy or hung.
        req @ Request::ReadMemory { .. } => unsafe { game::handle(&req) },
        req => {
            if !HOOKED.load(Ordering::SeqCst) {
                return Response::err("game hooks not installed (unsupported exe?); see status/logs");
            }
            let (tx, rx) = mpsc::channel();
            JOBS.lock().unwrap().push_back(Job { req, reply: tx });
            rx.recv_timeout(GAME_THREAD_TIMEOUT).unwrap_or_else(|_| {
                Response::err("timed out waiting for game thread (game frozen, minimized or loading?)")
            })
        }
    }
}

fn status() -> Response {
    // Plain reads of ints from another thread; good enough for a status snapshot.
    Response::ok(json!({
        "bridge_version": env!("CARGO_PKG_VERSION"),
        "game_version": if SUPPORTED.load(Ordering::SeqCst) { "1.0 US" } else { "unsupported" },
        "hooks_installed": HOOKED.load(Ordering::SeqCst),
        "frames_pumped": FRAMES_PUMPED.load(Ordering::Relaxed),
        "game_state": mem::read::<i32>(addr::GAME_STATE),
        "player_ped": mem::read::<u32>(addr::PLAYERS).map(|p| format!("{p:#x}")),
    }))
}

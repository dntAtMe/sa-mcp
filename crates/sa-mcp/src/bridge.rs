//! Client for the in-game bridges. One short-lived connection per request keeps this
//! robust against game restarts. Instances are discovered through their slot mutexes.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use proto::{instance_mutex_name, Request, Response, BASE_PORT, MAX_INSTANCES};
use windows::core::HSTRING;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};

/// Instance slots whose mutex currently exists, i.e. running games with the bridge loaded.
pub fn live_instances() -> Vec<u8> {
    (0..MAX_INSTANCES)
        .filter(|&n| {
            let name = HSTRING::from(instance_mutex_name(n));
            match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &name) } {
                Ok(h) => {
                    unsafe {
                        let _ = CloseHandle(h);
                    }
                    true
                }
                Err(_) => false,
            }
        })
        .collect()
}

pub fn resolve(instance: Option<u8>) -> Result<u8, String> {
    let live = live_instances();
    match instance {
        Some(n) if live.contains(&n) => Ok(n),
        Some(n) => Err(format!("instance {n} is not running (live: {live:?})")),
        None => live.first().copied().ok_or_else(|| {
            "no game instance with sa_bridge.asi is running. Use launch_instances to start one.".to_string()
        }),
    }
}

pub fn send(instance: Option<u8>, req: &Request) -> Result<Response, String> {
    send_timeout(instance, req, Duration::from_secs(5))
}

pub fn send_timeout(instance: Option<u8>, req: &Request, timeout: Duration) -> Result<Response, String> {
    let n = resolve(instance)?;
    let addr = SocketAddr::from(([127, 0, 0, 1], BASE_PORT + n as u16));
    let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(1000))
        .map_err(|e| format!("instance {n}: cannot reach bridge on {addr} ({e})"))?;
    stream.set_read_timeout(Some(timeout)).ok();
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    let mut line = serde_json::to_string(req).map_err(|e| e.to_string())?;
    line.push('\n');
    writer.write_all(line.as_bytes()).map_err(|e| format!("send failed: {e}"))?;

    let mut resp = String::new();
    BufReader::new(stream).read_line(&mut resp).map_err(|e| format!("instance {n}: receive failed: {e}"))?;
    serde_json::from_str(&resp).map_err(|e| format!("bad response from bridge: {e}: {resp}"))
}

/// Like `send` but unwraps `ok`/`error` into a Result over the data payload.
pub fn query(instance: Option<u8>, req: &Request) -> Result<serde_json::Value, String> {
    let r = send(instance, req)?;
    if r.ok {
        Ok(r.data.unwrap_or(serde_json::Value::Null))
    } else {
        Err(r.error.unwrap_or_else(|| "unknown bridge error".into()))
    }
}

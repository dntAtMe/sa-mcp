//! Client for the in-game bridge. One short-lived connection per request keeps this
//! robust against game restarts.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use proto::{Request, Response, BRIDGE_PORT};

pub fn send(req: &Request) -> Result<Response, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], BRIDGE_PORT));
    let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).map_err(|e| {
        format!(
            "cannot reach sa-bridge on {addr} ({e}). Is gta_sa.exe running with sa_bridge.asi installed? \
             Use launch_game to start it."
        )
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    let mut line = serde_json::to_string(req).map_err(|e| e.to_string())?;
    line.push('\n');
    writer.write_all(line.as_bytes()).map_err(|e| format!("send failed: {e}"))?;

    let mut resp = String::new();
    BufReader::new(stream).read_line(&mut resp).map_err(|e| format!("receive failed: {e}"))?;
    serde_json::from_str(&resp).map_err(|e| format!("bad response from bridge: {e}: {resp}"))
}

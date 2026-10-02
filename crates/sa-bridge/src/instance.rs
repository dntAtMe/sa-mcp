//! Instance slots: each running game claims the first free named mutex
//! `Local\sa-mcp-bridge-N`, which fixes its port (BASE_PORT + N) and log file. The MCP
//! server discovers live instances by probing the same mutex names.

use std::sync::OnceLock;

use proto::{instance_mutex_name, MAX_INSTANCES};
use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;

static INSTANCE: OnceLock<u8> = OnceLock::new();

/// Claims a slot. Returns None when all slots are taken. The mutex handle is intentionally
/// leaked: it must live as long as the process.
pub fn claim() -> Option<u8> {
    for n in 0..MAX_INSTANCES {
        let name = HSTRING::from(instance_mutex_name(n));
        let Ok(handle) = (unsafe { CreateMutexW(None, false, &name) }) else { continue };
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                let _ = CloseHandle(handle);
            }
            continue;
        }
        let _ = INSTANCE.set(n);
        return Some(n);
    }
    None
}

pub fn id() -> Option<u8> {
    INSTANCE.get().copied()
}

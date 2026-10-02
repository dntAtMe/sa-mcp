//! Fault-tolerant memory access for arbitrary addresses (agent-supplied or unverified).
//! Uses Read/WriteProcessMemory on our own process so a bad address returns an error
//! instead of crashing the game.

use std::ffi::c_void;
use std::mem::{size_of, MaybeUninit};

use windows::Win32::System::Diagnostics::Debug::{
    FlushInstructionCache, ReadProcessMemory, WriteProcessMemory,
};
use windows::Win32::System::Threading::GetCurrentProcess;

pub fn read_bytes(address: u32, len: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let mut read = 0usize;
    let ok = unsafe {
        ReadProcessMemory(
            GetCurrentProcess(),
            address as *const c_void,
            buf.as_mut_ptr() as *mut c_void,
            len,
            Some(&mut read),
        )
    };
    (ok.is_ok() && read == len).then_some(buf)
}

pub fn read<T: Copy>(address: u32) -> Option<T> {
    let mut out = MaybeUninit::<T>::uninit();
    let mut read = 0usize;
    let ok = unsafe {
        ReadProcessMemory(
            GetCurrentProcess(),
            address as *const c_void,
            out.as_mut_ptr() as *mut c_void,
            size_of::<T>(),
            Some(&mut read),
        )
    };
    (ok.is_ok() && read == size_of::<T>()).then(|| unsafe { out.assume_init() })
}

/// WriteProcessMemory temporarily lifts page protection itself, so this also works on code.
pub fn write_bytes(address: u32, bytes: &[u8]) -> Result<(), String> {
    let mut written = 0usize;
    unsafe {
        let proc = GetCurrentProcess();
        WriteProcessMemory(
            proc,
            address as *const c_void,
            bytes.as_ptr() as *const c_void,
            bytes.len(),
            Some(&mut written),
        )
        .map_err(|e| format!("WriteProcessMemory failed at {address:#x}: {e}"))?;
        let _ = FlushInstructionCache(proc, Some(address as *const c_void), bytes.len());
    }
    if written != bytes.len() {
        return Err(format!("partial write: {written}/{} bytes", bytes.len()));
    }
    Ok(())
}

/// Writes a plain value (fault-tolerant, works on code pages too).
pub fn write<T: Copy>(address: u32, value: T) -> Result<(), String> {
    let bytes = unsafe { std::slice::from_raw_parts(&value as *const T as *const u8, size_of::<T>()) };
    write_bytes(address, bytes)
}

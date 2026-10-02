//! Minimal in-process SDK for GTA San Andreas 1.0 US plugins (ASI / injected DLLs).
//!
//! - [`addr`]: addresses and struct offsets, each marked with how it was verified
//! - [`mem`]: fault-tolerant reads/writes (ReadProcessMemory on our own process)
//! - [`hook`]: call-site hooks that chain to the previous target (composes with other plugins)
//! - [`script`]: execute SCM opcodes through a private CRunningScript
//! - [`world`]: player/entity/pool helpers built on the above
//!
//! Game-state functions must be called on the game thread.

pub mod addr;
pub mod hook;
pub mod mem;
pub mod script;
pub mod world;

/// True when running inside gta_sa.exe 1.0 US (the only supported version).
pub fn is_supported_game() -> bool {
    mem::read::<u32>(addr::VERSION_CHECK) == Some(addr::VERSION_CHECK_US10)
}

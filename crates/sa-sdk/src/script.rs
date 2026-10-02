//! SCM command executor: a private CRunningScript driven one command at a time through
//! `CRunningScript::ProcessOneCommand`, the same technique plugin-sdk's `Command<>` uses.
//! The script object (and its 32 local variables) persists per process, so handles returned
//! by one call can be used by the next.
//!
//! Must be called on the game thread.

use std::sync::Mutex;

use crate::addr::*;

#[derive(Debug, Clone)]
pub enum Arg {
    Int(i32),
    Float(f32),
    /// Local variable slot 0-31 (input or output).
    Var(u16),
    Str(String),
}

#[derive(Debug, Clone)]
pub struct Command {
    pub op: u16,
    pub args: Vec<Arg>,
}

/// Shorthand constructor.
pub fn cmd(op: u16, args: Vec<Arg>) -> Command {
    Command { op, args }
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// (opcode, condition result) per executed command.
    pub conds: Vec<(u16, bool)>,
    /// Raw values of every local var referenced by the commands, by slot.
    pub vars: Vec<(usize, u32)>,
}

type InitFn = unsafe extern "thiscall" fn(*mut u8);
type ProcessOneFn = unsafe extern "thiscall" fn(*mut u8) -> u8;

/// Zero padding after each command so a too-short argument list reads "end of args"
/// instead of running off the buffer.
const CODE_PADDING: usize = 32;

struct Ctx {
    script: Box<[u32; SCRIPT_SIZE / 4]>,
}

// The raw script memory is only touched on the game thread.
unsafe impl Send for Ctx {}

static CTX: Mutex<Option<Ctx>> = Mutex::new(None);

fn encode(cmd: &Command) -> Result<Vec<u8>, String> {
    let mut code = cmd.op.to_le_bytes().to_vec();
    for arg in &cmd.args {
        match arg {
            Arg::Int(v) => {
                code.push(0x01);
                code.extend_from_slice(&v.to_le_bytes());
            }
            Arg::Float(v) => {
                code.push(0x06);
                code.extend_from_slice(&v.to_le_bytes());
            }
            Arg::Var(i) => {
                if *i as usize >= SCRIPT_LOCAL_COUNT {
                    return Err(format!("var {i} out of range 0-{}", SCRIPT_LOCAL_COUNT - 1));
                }
                code.push(0x03);
                code.extend_from_slice(&i.to_le_bytes());
            }
            Arg::Str(s) => {
                let bytes = s.as_bytes();
                if bytes.len() > 127 {
                    return Err("string args are limited to 127 bytes".into());
                }
                code.push(0x0E);
                code.push(bytes.len() as u8);
                code.extend_from_slice(bytes);
            }
        }
    }
    Ok(code)
}

unsafe fn ensure_ctx(slot: &mut Option<Ctx>) -> *mut u8 {
    if slot.is_none() {
        let mut script = Box::new([0u32; SCRIPT_SIZE / 4]);
        let init: InitFn = std::mem::transmute(RUNNING_SCRIPT_INIT as usize);
        init(script.as_mut_ptr() as *mut u8);
        let name = script.as_mut_ptr() as *mut u8;
        std::ptr::copy_nonoverlapping(b"sa-sdk\0".as_ptr(), name.add(8), 7);
        *slot = Some(Ctx { script });
    }
    slot.as_mut().unwrap().script.as_mut_ptr() as *mut u8
}

unsafe fn local(script: *mut u8, i: usize) -> u32 {
    *(script.add(SCRIPT_LOCALS + i * 4) as *const u32)
}

/// Runs commands in order. Stops with an error at the first command whose argument encoding
/// does not match what the game consumed (wrong argument count/types for that opcode).
///
/// # Safety
/// Game thread only. Bad argument values can crash the game.
pub unsafe fn run(cmds: &[Command]) -> Result<Outcome, String> {
    let mut guard = CTX.lock().map_err(|_| "script context poisoned")?;
    let script = ensure_ctx(&mut guard);
    let process: ProcessOneFn = std::mem::transmute(RUNNING_SCRIPT_PROCESS_ONE as usize);

    let mut out = Outcome::default();
    let mut touched = std::collections::BTreeSet::new();
    for (n, cmd) in cmds.iter().enumerate() {
        let code = encode(cmd)?;
        let mut buf = code.clone();
        buf.resize(code.len() + CODE_PADDING, 0);
        for a in &cmd.args {
            if let Arg::Var(i) = a {
                touched.insert(*i as usize);
            }
        }

        let start = buf.as_ptr() as u32;
        *(script.add(SCRIPT_BASE_IP) as *mut u32) = start;
        *(script.add(SCRIPT_IP) as *mut u32) = start;
        *(script.add(SCRIPT_SP) as *mut u16) = 0;
        *script.add(SCRIPT_NOT_FLAG) = 0;
        *script.add(SCRIPT_IS_MISSION) = 0;
        process(script);
        let consumed = (*(script.add(SCRIPT_IP) as *const u32)).wrapping_sub(start) as usize;

        out.conds.push((cmd.op, *script.add(SCRIPT_COND_RESULT) != 0));
        if consumed != code.len() {
            return Err(format!(
                "command #{n} ({:04X}): game consumed {consumed} bytes but {} were encoded -- \
                 wrong argument count/types for this opcode (or it jumped). Stopped here.",
                cmd.op,
                code.len(),
            ));
        }
    }
    out.vars = touched.into_iter().map(|i| (i, local(script, i))).collect();
    Ok(out)
}

/// Reads local var `i` of the persistent context (0 when it was never created).
///
/// # Safety
/// Game thread only.
pub unsafe fn var(i: usize) -> u32 {
    let mut guard = CTX.lock().unwrap();
    if i >= SCRIPT_LOCAL_COUNT || guard.is_none() {
        return 0;
    }
    local(ensure_ctx(&mut guard), i)
}

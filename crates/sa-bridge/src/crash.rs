//! Diagnostic: logs the first few access violations (first-chance) with registers and
//! a stack dump to sa_bridge.log. Never handles the exception.

use std::sync::atomic::{AtomicU32, Ordering};

use std::ffi::c_void;

use windows::Win32::Foundation::EXCEPTION_ACCESS_VIOLATION;
use windows::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION};
use windows::Win32::System::Diagnostics::Debug::{AddVectoredExceptionHandler, EXCEPTION_POINTERS};

use crate::{log, mem};

const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
const MAX_REPORTS: u32 = 8;
static REPORTS: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn handler(info: *mut EXCEPTION_POINTERS) -> i32 {
    let rec = &*(*info).ExceptionRecord;
    if rec.ExceptionCode != EXCEPTION_ACCESS_VIOLATION || REPORTS.fetch_add(1, Ordering::SeqCst) >= MAX_REPORTS {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    let ctx = &*(*info).ContextRecord;
    let stack: Vec<String> = (0..24)
        .map(|i| mem::read::<u32>(ctx.Esp + i * 4).map_or("????????".into(), |v| format!("{v:08x}")))
        .collect();
    log::write(&format!(
        "AV at eip={:#010x} kind={} addr={:#x} [{}]; eax={:08x} ebx={:08x} ecx={:08x} edx={:08x} esi={:08x} edi={:08x} ebp={:08x} esp={:08x} thread={}\n  stack: {}",
        ctx.Eip,
        rec.ExceptionInformation[0],
        rec.ExceptionInformation[1],
        describe_page(rec.ExceptionInformation[1] as u32),
        ctx.Eax, ctx.Ebx, ctx.Ecx, ctx.Edx, ctx.Esi, ctx.Edi, ctx.Ebp, ctx.Esp,
        windows::Win32::System::Threading::GetCurrentThreadId(),
        stack.join(" "),
    ));
    EXCEPTION_CONTINUE_SEARCH
}

fn describe_page(address: u32) -> String {
    let mut mbi = MEMORY_BASIC_INFORMATION::default();
    let n = unsafe { VirtualQuery(Some(address as *const c_void), &mut mbi, std::mem::size_of::<MEMORY_BASIC_INFORMATION>()) };
    if n == 0 {
        return "VirtualQuery failed".into();
    }
    format!(
        "alloc_base={:#x} state={:#x} protect={:#x} type={:#x}",
        mbi.AllocationBase as usize, mbi.State.0, mbi.Protect.0, mbi.Type.0
    )
}

pub fn install() {
    unsafe { AddVectoredExceptionHandler(1, Some(handler)) };
}

//! x86_64 task context and context switch.
//!
//! # Saved state
//!
//! The System V AMD64 ABI designates the following registers callee-saved: `rbx`,
//! `rbp`, `r12`, `r13`, `r14`, `r15`, `rsp`.
//!
//! `rip` is not saved explicitly, it is captured implicitly as the return address pushed
//! by `call switch_to_asm`. When the next task is restored, `ret` pops that address and resumes
//! execution there.

use crate::task::TaskContext;

/// Offsets into [`X86_64Context`] used by the assembly stub.
///
/// These must stay in sync with the field order in [`X86_64Context`].
/// Verified by compile-time assertions below.
mod offset {
    pub const RSP: usize = 0;
}

/// Saved callee-saved register state for an x86_64 task.
///
/// The layout is fixed (`repr(C)`) because the assembly stub accesses
/// `rsp` at a known byte offset from the struct pointer.
#[derive(Debug, Default)]
#[repr(C)]
pub struct X86_64Context {
    /// Saved stack pointer.
    ///
    /// This is the only field accessed directly by the assembly stub; all
    /// other callee-saved registers are pushed/popped on the task's own stack
    /// (pointed to by `rsp`), which is the most cache-friendly layout.
    rsp: u64,
}

const _: () = assert!(
    core::mem::offset_of!(X86_64Context, rsp) == offset::RSP,
    "X86_64Context::rsp offset mismatch. update offset::RSP",
);

impl TaskContext for X86_64Context {
    unsafe fn new(entry: fn() -> !, stack_top: *mut u8) -> Self {
        let mut sp = stack_top as *mut u64;

        unsafe {
            sp = sp.sub(1);
            sp.write(0);

            sp = sp.sub(1);
            sp.write(entry as u64);

            sp = sp.sub(1);
            sp.write(0u64); // rbx

            sp = sp.sub(1);
            sp.write(0u64); // rbp

            sp = sp.sub(1);
            sp.write(0u64); // r12

            sp = sp.sub(1);
            sp.write(0u64); // r13

            sp = sp.sub(1);
            sp.write(0u64); // r14

            sp = sp.sub(1);
            sp.write(0u64); // r15
        }

        let rsp_val = sp as u64;

        crate::println!("new_task: entry={:#x} rsp={:#x}", entry as u64, rsp_val);
        crate::println!("  [rsp+0]={:#x}  (should be r15=0)", unsafe {
            *(rsp_val as *const u64)
        });
        crate::println!("  [rsp+8]={:#x}  (should be r14=0)", unsafe {
            *((rsp_val + 8) as *const u64)
        });
        crate::println!("  [rsp+16]={:#x} (should be r13=0)", unsafe {
            *((rsp_val + 16) as *const u64)
        });
        crate::println!("  [rsp+24]={:#x} (should be r12=0)", unsafe {
            *((rsp_val + 24) as *const u64)
        });
        crate::println!("  [rsp+32]={:#x} (should be rbp=0)", unsafe {
            *((rsp_val + 32) as *const u64)
        });
        crate::println!("  [rsp+40]={:#x} (should be rbx=0)", unsafe {
            *((rsp_val + 40) as *const u64)
        });
        crate::println!("  [rsp+48]={:#x} (should be entry)", unsafe {
            *((rsp_val + 48) as *const u64)
        });
        Self { rsp: sp as u64 }
    }

    #[inline(always)]
    unsafe fn switch_to(current: *mut Self, next: *const Self) {
        unsafe {
            switch_to_asm(current, next);
        }
    }
}

/// The actual assembly context switch routine.
///
/// `rdi` = `*mut X86_64Context` (current)
/// `rsi` = `*const X86_64Context` (next)
///
/// # Safety
///
/// See [`TaskContext::switch_to`].
#[unsafe(naked)]
unsafe extern "C" fn switch_to_asm(current: *mut X86_64Context, next: *const X86_64Context) {
    // SAFETY: naked function.
    core::arch::naked_asm!(
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        "mov [rdi + {rsp_off}], rsp",
        "mov rsp, [rsi + {rsp_off}]",

        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        //
        // Return into the next task. `ret` pops the return address that was
        // either:
        //   a) pushed by the next task's earlier `call switch_to_asm`, or
        //   b) set up by `new` as the entry function pointer.
        "ret",
        rsp_off = const offset::RSP,
    );
}

//! x86_64 platform implementation.

mod entry {
    core::arch::global_asm!(include_str!("entry.s"));
}

pub mod gdt;
pub mod imp;

/// Enable the x87 FPU and SSE instruction sets.
///
/// Rust's `core` internals may emit SSE instructions, omitting this
/// call will cause a `#UD` fault.
///
/// Specifically, this clears `CR0.EM`, sets `CR0.MP` and sets
/// `CR4.OSFXSR` and `CR4.OSXMMEXCPT`.
#[inline(always)]
pub fn enable_sse() {
    unsafe {
        core::arch::asm!(
            "mov rax, cr0",
            "and ax, 0xFFFB",
            "or ax, 0x2",
            "mov cr0, rax",
            "mov rax, cr4",
            "or ax, 3 << 9",
            "mov cr4, rax",
            out("rax") _,
            options(nostack, nomem)
        );
    }
}

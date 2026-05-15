//! x86_64 platform implementation.

mod entry;

/// Halt the current CPU core.
///
/// Executes `hlt`, pauses the core until the next interrupt fires.
/// Always call this inside a loop.
#[inline]
pub fn halt() {
    // SAFETY: `hlt` is always safe to execute in ring 0.
    unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
}

/// Disable interrupts on the current core.
///
/// After this returns, no external interrupts will be delivered until
/// [`enable_interrupts`] is called or the core is reset.
#[inline]
pub fn disable_interrupts() {
    // SAFETY: `cli` is valid in ring 0 and has no memory side effects
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
}

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

//! x86_64 platform implementation.

mod entry;

/// Halt the current CPU core.
///
/// Executes `hlt`, which pauses the core until the next interrupt.
/// Should always be called inside a loop since an interrupt can wake
/// the core back up.
///
/// # Safety
///
/// Must be called with interrupts in a known state. In the kernel's
/// early boot context, interrupts are masked by Limine, so this is safe.
#[inline]
pub fn halt() {
    // SAFETY: `hlt` is always safe to execute in ring 0.
    unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
}

#[inline]
pub fn disable_interrupts() {
    // SAFETY: cli instruction is safe to execute in kernel mode.
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
}

/// Enable SSE and FPU instruction sets in the CPU.
#[inline(always)]
pub fn enable_sse() {
    // SAFETY: We are modifying CPU control registers (CR0 and CR4) to enable
    // SSE features (OSFXSR and OSXMMEXCPT). This is required because Rust's
    // core library and formatting macros use SSE instructions (like movaps)
    // for optimization. If we don't enable this, the CPU throws an #UD exception.
    unsafe {
        core::arch::asm!(
            "mov rax, cr0",
            "and ax, 0xFFFB", // EM bitini (Bit 2) temizle
            "or ax, 0x2",     // MP bitini (Bit 1) set et
            "mov cr0, rax",
            "mov rax, cr4",
            "or ax, 3 << 9",  // OSFXSR (Bit 9) ve OSXMMEXCPT (Bit 10) bitlerini set et!
            "mov cr4, rax",
            out("rax") _,
            options(nostack, nomem)
        );
    }
}

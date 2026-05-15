//! AArch64 platform implementation.

mod entry;

/// Halt the current CPU core.
///
/// Executes `wfi`, puts the core into a low-power state until
/// the next interrupt or event. Always call this inside a loop.
#[inline]
pub fn halt() {
    // SAFETY: `wfi` is always valid at EL1/EL2.
    unsafe { core::arch::asm!("wfi", options(nomem, nostack, preserves_flags)) };
}

/// Disable hardware interrupts on the current core.
///
/// Sets all four DAIF mask bits (Debug, SError, IRQ, FIQ). No external
/// interrupts will be delivereed until the bits are cleared again.
#[inline]
pub fn disable_interrupts() {
    // SAFETY: msr daifset is safe to execute in kernel mode (EL1).
    unsafe { core::arch::asm!("msr daifset, #0xf", options(nomem, nostack)) };
}

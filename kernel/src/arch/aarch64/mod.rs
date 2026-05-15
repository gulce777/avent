//! AArch64 platform implementation.

mod entry;

/// Halt the current CPU core.
///
/// Executs `wfi` (wait for interrupt), which puts the core into a
/// low-power state until the next interrupt or event.
/// Should always be called inside a loop since an interrupt can
/// resume exception.
///
/// # Safety
///
/// Safe to call at EL1/EL2 at any time. All interrupts are masked by
/// Limine at entry, so no spurious wakeup will escape the halt loop.
#[inline]
pub fn halt() {
    // SAFETY: `wfi` is always safe to execute at EL1/EL2.
    unsafe { core::arch::asm!("wfi", options(nomem, nostack, preserves_flags)) };
}

/// Disable hardware interrupts.
#[inline]
pub fn disable_interrupts() {
    // SAFETY: msr daifset is safe to execute in kernel mode (EL1).
    unsafe { core::arch::asm!("msr daifset, #0xf", options(nomem, nostack)) };
}

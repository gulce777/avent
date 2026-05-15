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

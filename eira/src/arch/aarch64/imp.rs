//! AArch64 implementation of the [`Arch`] trait.

use crate::{arch::Arch, mm::VirtAddr};

pub struct AArch64;

impl Arch for AArch64 {
    #[inline]
    fn halt() {
        // SAFETY: `wfi` is always valid at EL1/EL2.
        unsafe { core::arch::asm!("wfi", options(nomem, nostack, preserves_flags)) };
    }

    #[inline]
    fn disable_interrupts() {
        // SAFETY: msr daifset is safe to execute in kernel mode (EL1).
        unsafe { core::arch::asm!("msr daifset, #0xf", options(nomem, nostack)) };
    }

    fn init_cpu() {
        // TODO: load exception vector table.
    }

    fn flush_tlb_page(addr: VirtAddr) {
        unsafe {
            let va = addr.as_usize() >> 12;
            core::arch::asm!(
                "tlbi vaae1is, {va}",
                "dsb ish",
                "isb",
                va = in(reg) va,
                options(nostack),
            );
        }
    }
}

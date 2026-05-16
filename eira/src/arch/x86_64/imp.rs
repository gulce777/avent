//! x86_64 implementation of the [`Arch`] trait.

use super::gdt::CpuTables;
use super::idt::Idt;
use crate::arch::Arch;
use spin::Once;

static BSP_TABLES: CpuTables = CpuTables::new();
static IDT: Idt = Idt::new();

pub struct X86_64;

impl Arch for X86_64 {
    /// Executes `hlt`. Pauses the core until the next interrupt fires.
    #[inline]
    fn halt() {
        // SAFETY: `hlt` is always valid in ring 0.
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }

    /// Executes `cli`, masks all external interrupts on the current core.
    #[inline]
    fn disable_interrupts() {
        // SAFETY: `cli` is valid in ring 0 and has no memory side effects.
        unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
    }

    fn init_cpu() {
        unsafe { BSP_TABLES.load() };

        unsafe { IDT.load() };
    }
}

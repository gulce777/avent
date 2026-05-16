//! x86_64 implementation of the [`Arch`] trait.

use super::gdt::CpuTables;
use super::idt::Idt;
use crate::arch::Arch;
use crate::mm::VirtAddr;

static BSP_TABLES: CpuTables = CpuTables::new();
static IDT: Idt = Idt::new();

pub struct X86_64;

impl Arch for X86_64 {
    #[inline]
    fn halt() {
        // SAFETY: valid in ring 0.
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }

    #[inline]
    fn disable_interrupts() {
        // SAFETY: valid in ring 0.
        unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
    }

    fn init_cpu() {
        unsafe { BSP_TABLES.load() };

        unsafe { IDT.load() };
    }

    fn flush_tlb_page(addr: VirtAddr) {
        unsafe {
            core::arch::asm!(
                "invlpg [{addr}]",
                addr = in(reg) addr.as_usize(),
                options(nostack, preserves_flags),
            );
        }
    }

    #[cfg(feature = "kernel-tests")]
    fn new_test_mapper() -> impl crate::mm::paging::Mapper {
        let pml4 = crate::mm::allocate();
        let hhdm = crate::mm::init::hhdm_offset();
        unsafe {
            let base = pml4.base();
            let ptr = (base.as_usize() + hhdm) as *mut u8;
            core::ptr::write_bytes(ptr, 0, crate::mm::PAGE_SIZE);
            let raw_pml4 = pml4.into_inner();
            super::paging::mapper::PageTableMapper::new(raw_pml4, hhdm)
        }
    }
}

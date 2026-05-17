//! x86_64 implementation of the [`Arch`] trait.

use super::gdt::CpuTables;
use super::idt::Idt;
use crate::arch::Arch;
use crate::mm::VirtAddr;

static BSP_TABLES: CpuTables = CpuTables::new();
static IDT: Idt = Idt::new();

pub struct X86_64;

impl Arch for X86_64 {
    type Mapper = super::paging::mapper::PageTableMapper;

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

    fn enable_interrupts() {
        // SAFETY: valid in ring 0.
        unsafe {
            core::arch::asm!("sti", options(nomem, nostack));
        }
    }

    fn init_cpu() {
        unsafe { BSP_TABLES.load() };

        unsafe { IDT.load() };

        unsafe {
            super::pic::disable_lapic();
            super::pic::init();
            super::pic::init_pit();
        };
    }

    fn register_irq(irq: u32, handler: fn()) {
        assert!(irq <= u8::MAX as u32, "x86_64 IRQ number too large");
        crate::arch::x86_64::irq::register_irq(irq as u8, handler);
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

    unsafe fn create_mapper(root: crate::mm::OwnedFrame) -> Self::Mapper {
        let hhdm = crate::mm::init::hhdm_offset();

        unsafe {
            let ptr = (root.base().as_usize() + hhdm) as *mut u8;
            core::ptr::write_bytes(ptr, 0, crate::mm::PAGE_SIZE);
        }

        let raw_frame = unsafe { root.into_inner() };

        // SAFETY: `raw_frame` is zeroed, page-aligned and exclusively owned.
        unsafe { super::paging::mapper::PageTableMapper::new(raw_frame, hhdm) }
    }

    unsafe fn mapper_from_active(root_phys: crate::mm::PhysAddr) -> Self::Mapper {
        use crate::mm::PhysFrame;

        let hhdm = crate::mm::init::hhdm_offset();

        let raw_frame =
            PhysFrame::from_base(root_phys).expect("active PML4 base address must be page-aligned");

        // SAFETY: forwarded from caller. The bootloader guarantees a valid,
        // loaded PML4 at `root_phys`.
        unsafe { super::paging::mapper::PageTableMapper::new(raw_frame, hhdm) }
    }

    #[cfg(feature = "kernel-tests")]
    fn new_test_mapper() -> impl crate::mm::paging::Mapper {
        let pml4 = crate::mm::allocate();
        let hhdm = crate::mm::init::hhdm_offset();
        // SAFETY: fresh frame, correct HHDM. see `create_mapper`
        unsafe { Self::create_mapper(pml4) }
    }

    fn active_page_table() -> crate::mm::PhysAddr {
        let cr3: usize;
        unsafe {
            core::arch::asm!(
                "mov {}, cr3",
                out(reg) cr3,
                options(nomem, nostack, preserves_flags)
            );
        }

        crate::mm::PhysAddr::new_truncate(cr3 & !0xFFF)
    }
}

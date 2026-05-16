//! x86_64 GDT & TSS.
//!
//! # Layout
//!
//! Each logical CPU owns one [`CpuTables`] intance, which contains:
//!
//! - A 4-entry GDT: null, kernel code (64-bit), kernel data, TSS (16-byte system descriptor)
//! - A TSS with all 7 IST stacks + 1 RSP0 stack, each [`IST_STACK_SIZE`] bytes, allocated from
//!   the physical frame allocator and mapped via the HHDM
//!
//! # Per-CPU ownership
//!
//! [`CpuTables`] is not `Copy` and must not be moved after [`CpuTables::load`] is called. The CPU
//! holds a pointer to the GDT and TSS for as longs as they are loaded. Pin it in place before calling
//! `load`.
//!
//! # IST assignment
//!
//! | IST index | Purpose             |
//! |-----------|---------------------|
//! | 1 (DF)    | `#DF` double fault  |
//! | 2 (NMI)   | NMI                 |
//! | 3 (MCE)   | `#MC` machine check |
//! | 4 (SS)    | `#SS` stack segment |
//! | 5 (GP)    | `#GP` general prot. |
//! | 6 (PF)    | `#PF` page fault    |
//! | 7 (BP)    | `#BP` breakpoint    |
//!
//! Indices are exported as constants for use when building the IDT.

use core::cell::UnsafeCell;
use core::mem::size_of;

/// Size of each IST / RSP0 stack in bytes (16 KiB).
///
/// 16 KiB is enough for deep exception handler call chains while staying
/// well withing the physical frame budget during early boot.
pub const IST_STACK_SIZE: usize = 16 * 1024;

/// Total number of IST slots defined by the x86_64 ABI.
pub const IST_COUNT: usize = 7;

/// IST slot index for `#DF` (double fault). **1-based** as the hardware expects.
pub const IST_DF: u8 = 1;
/// IST slot index for NMI.
pub const IST_NMI: u8 = 2;
/// IST slot index for `#MC` (machine check).
pub const IST_MCE: u8 = 3;
/// IST slot index for `#SS` (stack-segment fault).
pub const IST_SS: u8 = 4;
/// IST slot index for `#GP` (general protection fault).
pub const IST_GP: u8 = 5;
/// IST slot index for `#PF` (page fault).
pub const IST_PF: u8 = 6;
/// IST slot index for `#BP` (breakpoint).
pub const IST_BP: u8 = 7;

/// Kernel code segment selector (GDT index 1, RPL 0).
pub const KCODE_SELECTOR: u16 = 1 << 3;
/// Kernel data segment selector (GDT index 2, RPL 0).
pub const KDATA_SELECTOR: u16 = 2 << 3;
/// TSS segment selector (GDT index 3, RPL 0).
pub const TSS_SELECTOR: u16 = 3 << 3;

/// Descriptor present bit.
const PRESENT: u64 = 1 << 47;
/// Descriptor privilege level 0 (kernel).
const DPL0: u64 = 0 << 45;
/// Code/data descriptor type (S = 1).
const CODE_DATA_TYPE: u64 = 1 << 44;
/// 64-bit code segment (L bit).
const LONG_MODE: u64 = 1 << 53;
/// Executable bit (code segment).
const EXECUTABLE: u64 = 1 << 43;
/// Read/write bit (data or readable code).
const RW: u64 = 1 << 41;
/// System descriptor type (S = 0), used for TSS.
const SYSTEM_TYPE: u64 = 0 << 44;
/// Available TSS (type = 0b1001).
const TSS_AVAILABLE: u64 = 0x9 << 40;

/// 64-bit kernel code descriptor.
///
/// Executable, readable, long-mode, DPL0.
const KCODE_DESC: u64 = PRESENT | DPL0 | CODE_DATA_TYPE | EXECUTABLE | RW | LONG_MODE;

/// Kernel data descriptor.
///
/// Readable/writable, DPL 0. In 64-bit mode the base/limit are ignored.
const KDATA_DESC: u64 = PRESENT | DPL0 | CODE_DATA_TYPE | RW;

/// x86_64 TSS.
///
/// Only `rsp0` and the seven IST entries are used in 64-bit mode.
/// All other fields are reserved/ignored.
#[derive(Debug)]
#[repr(C, packed)]
pub struct Tss {
    _reserved0: u32,
    /// Privilege-level 0 stack pointer. Loaded by the CPU on ring-3 -> ring-0
    /// transitions (syscalls, hardware interrupts while in user mode).
    pub rsp0: u64,
    _rsp1: u64,
    _rsp2: u64,
    _reserved1: u64,
    /// IST entries (1-based; index 0 is unused by hardware).
    ///
    /// `ist[0]` corresponds to IST1, `ist[6]` to IST7.
    pub ist: [u64; IST_COUNT],
    _reserved2: u64,
    _reserved3: u16,
    /// I/O map base address. Set to `size_of::<Tss>()` to indicate no I/O map.
    pub iomap_base: u16,
}

impl Tss {
    /// Returns a zeroed TSS with `iomap_base` set correctly.
    pub const fn new() -> Self {
        Self {
            _reserved0: 0,
            rsp0: 0,
            _rsp1: 0,
            _rsp2: 0,
            _reserved1: 0,
            ist: [0; IST_COUNT],
            _reserved2: 0,
            _reserved3: 0,
            iomap_base: size_of::<Tss>() as u16,
        }
    }
}

impl Default for Tss {
    fn default() -> Self {
        Self::new()
    }
}

/// A minimal 64-bit GDT: null, kernel code, kernel data, TSS (2 entries).
///
/// The TSS descriptor is 16 bytes (two consecutive `u64` slots) in 64-bit mode.
#[derive(Debug)]
#[repr(C, align(16))]
pub struct Gdt {
    null: u64,
    kcode: u64,
    kdata: u64,
    tss_low: u64,
    tss_high: u64,
}

impl Gdt {
    /// Build a GDT with kernel/data descriptors.
    ///
    /// The TSS descriptor slots are left zeroed, call [`Gdt::set_tss`] to fill
    /// them in after the TSS address is known.
    pub const fn new() -> Self {
        Self {
            null: 0,
            kcode: KCODE_DESC,
            kdata: KDATA_DESC,
            tss_low: 0,
            tss_high: 0,
        }
    }

    /// Write the 16-byte TSS system descriptor for `tss` into the GDT.
    ///
    /// Must be called before [`CpuTables::load`].
    ///
    /// # Safety
    ///
    /// `tss` must remain valid (not moved, not dropped) as long as the GDT is
    /// loaded on any CPU.
    pub unsafe fn set_tss(&mut self, tss: &Tss) {
        let base = tss as *const Tss as u64;
        let limit = (size_of::<Tss>() - 1) as u64;

        // Low 8 bytes of a system descriptor:
        //   [15:0]  limit[15:0]
        //   [39:16] base[23:0]
        //   [43:40] type (1001 = available TSS)
        //   [44]    S=0 (system)
        //   [46:45] DPL=0
        //   [47]    P=1
        //   [51:48] limit[19:16]
        //   [55:52] flags(G=0, 0, 0, AVL=0)
        //   [63:56] base[31:24]
        let low = (limit & 0xFFFF)
            | ((base & 0xFF_FFFF) << 16)
            | SYSTEM_TYPE
            | TSS_AVAILABLE
            | PRESENT
            | ((limit >> 16) << 48)
            | (((base >> 24) & 0xFF) << 56);

        let high = base >> 32;

        self.tss_low = low;
        self.tss_high = high;
    }

    /// Returns a [`GdtPointer`] suitable for `lgdt`.
    fn pointer(&self) -> GdtPointer {
        GdtPointer {
            limit: (size_of::<Gdt>() - 1) as u16,
            base: self as *const Gdt as u64,
        }
    }
}

impl Default for Gdt {
    fn default() -> Self {
        Self::new()
    }
}

/// The 10-byte value loaded by the `lgdt` instruction.
#[repr(C, packed)]
struct GdtPointer {
    limit: u16,
    base: u64,
}

/// Storage for all per-CPU exception stacks.
///
/// One stack for RSP0 (ring-0 entry from user mode) and one per IST slot.
/// Each stack is [`IST_STACK_SIZE`] bytes, 16-byte aligned as required by the
/// System V ABI.
#[repr(C, align(16))]
pub struct CpuStacks {
    /// Stacks for IST1..=IST7, indexed 0..IST_COUNT.
    pub ist: [[u8; IST_STACK_SIZE]; IST_COUNT],
    /// Stack used when entering ring-0 from ring-3 (RSP0).
    pub rsp0: [u8; IST_STACK_SIZE],
}

impl CpuStacks {
    pub const fn new() -> Self {
        Self {
            ist: [[0u8; IST_STACK_SIZE]; IST_COUNT],
            rsp0: [0u8; IST_STACK_SIZE],
        }
    }
}

impl Default for CpuStacks {
    fn default() -> Self {
        Self::new()
    }
}

/// All GDT-related CPU state for one logical processor.
///
/// # Pinning requirement
///
/// **Do not move this struct after calling [`load`](CpuTables::load).**
/// The `lgdt` and `ltr` instructions store raw pointers into this struct.
/// Moving it invalidates those pointers and causes undefined behaviour.
pub struct CpuTables {
    gdt: UnsafeCell<Gdt>,
    tss: UnsafeCell<Tss>,
    stacks: CpuStacks,
}

// SAFETY: After `call_once` completes the contents are only accessed from
// the owning CPU. The `spin::Once` serialises the single initialisation.
unsafe impl Sync for CpuTables {}

impl CpuTables {
    /// Allocate and initialise per-CPU tables.
    ///
    /// All IST stacks and RSP0 are wired up before returning. The GDT and TSS
    /// are fully populated and ready for [`load`](CpuTables::load).
    pub const fn new() -> Self {
        Self {
            gdt: UnsafeCell::new(Gdt::new()),
            tss: UnsafeCell::new(Tss::new()),
            stacks: CpuStacks::new(),
        }
    }

    /// Load this CPU's GDT, reload segment registers, and install the TSS.
    ///
    /// Must be called exactly once per logical CPU, after pinning `self` in
    /// place (the `Box` returned by [`new`](CpuTables::new) satisfies this).
    ///
    /// # Safety
    ///
    /// - `self` must not be moved or dropped while loaded on the CPU.
    /// - Must be called from ring-0 (CPL 0).
    pub unsafe fn load(&self) {
        // SAFETY: Exclusive logical access during boot init. No other CPU
        // holds a reference to these tables yet.
        let tss = unsafe { &mut *self.tss.get() };
        let gdt = unsafe { &mut *self.gdt.get() };

        for i in 0..IST_COUNT {
            let stack_top = self.stacks.ist[i].as_ptr() as u64 + IST_STACK_SIZE as u64;
            tss.ist[i] = stack_top;
        }
        tss.rsp0 = self.stacks.rsp0.as_ptr() as u64 + IST_STACK_SIZE as u64;

        // Encode TSS descriptor now that `tss` is at its final address.
        // SAFETY: `tss` is inside `self` which is pinned in the Once static.
        unsafe { gdt.set_tss(tss) };

        let ptr = gdt.pointer();

        unsafe {
            core::arch::asm!(
                "lgdt [{ptr}]",
                ptr = in(reg) &ptr,
                options(nostack, preserves_flags),
            );

            core::arch::asm!(
                "push {kcode}",
                "lea {tmp}, [rip + 2f]",
                "push {tmp}",
                "retfq",
                "2:",
                kcode = const KCODE_SELECTOR,
                tmp = out(reg) _,
                options(preserves_flags),
            );

            core::arch::asm!(
                "mov ax, {kdata}",
                "mov ds, ax",
                "mov es, ax",
                "mov fs, ax",
                "mov gs, ax",
                "mov ss, ax",
                kdata = const KDATA_SELECTOR,
                out("ax") _,
                options(nostack, preserves_flags),
            );

            core::arch::asm!(
                "ltr ax",
                in("ax") TSS_SELECTOR,
                options(nostack, preserves_flags),
            );
        }

        log::debug!(
            "gdt/tss loaded on cpu (gdt={:#p}, tss={:#p})",
            &self.gdt,
            &self.tss,
        );
    }

    /// Returns a reference to this CPU's TSS.
    ///
    /// # Safety
    ///
    /// Must not be called concurrently with [`load`](CpuTables::load).
    #[inline]
    pub unsafe fn tss(&self) -> &Tss {
        unsafe { &*self.tss.get() }
    }

    /// Returns a mutable reference to this CPU's TSS.
    ///
    /// # Safety
    ///
    /// Must not be called concurrently with any other accessor.
    #[inline]
    pub unsafe fn tss_mut(&mut self) -> &mut Tss {
        unsafe { &mut *self.tss.get() }
    }
}

impl Default for CpuTables {
    fn default() -> Self {
        Self {
            gdt: UnsafeCell::new(Gdt::new()),
            tss: UnsafeCell::new(Tss::new()),
            stacks: CpuStacks::new(),
        }
    }
}

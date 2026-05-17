//! Architecture-specific implementations.
//!
//! Exposes a unified [`Arch`] trait implemented by every supported target.
//! Call sites use the [`Platform`] type alias and never depend on a concrete
//! backend directly.
//!
//! # Supported architectures
//!
//! - `x86_64`
//! - `aarch64`

use crate::mm::VirtAddr;

#[cfg(target_arch = "x86_64")]
mod x86_64;
#[cfg(target_arch = "x86_64")]
pub use x86_64::enable_sse;
#[cfg(target_arch = "x86_64")]
use x86_64::imp::X86_64 as PlatformImpl;

#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(target_arch = "aarch64")]
use aarch64::imp::AArch64 as PlatformImpl;

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("unsupported architecture, supported targets: x86_64, aarch64");

/// The current platform's architecture backend.
///
/// A compile-time alias for the concrete [`Arch`] implementor selected for
/// the target architecture. Always prefer `Platform` over the underlying type.
pub type Platform = PlatformImpl;

/// The current platform's concrete page table mapper.
pub type PlatformMapper = <Platform as Arch>::Mapper;

/// The interface every architecture backend must implement.
///
/// All methods are static. The implementing type is always zero-sized struct
/// with no runtime representation.
pub trait Arch {
    type Mapper: crate::mm::paging::Mapper;

    /// Halt the current CPU core until the next interrupt or event.
    ///
    /// Always call this inside a loop, the core will resume execution
    /// after any interrupt fires.
    fn halt();

    /// Disable all hardware interrupts on the current core.
    ///
    /// After this returns, no external interrupts will be delivered until
    /// they are explicitly re-enabled.
    fn disable_interrupts();

    /// Perform one-time, per-CPU hardware initialisation.
    ///
    /// Called once per logical CPU during early boot, before interrupts
    /// are enabled.
    fn init_cpu();

    /// Invalidate the TLB entry for a single virtual address on the current core.
    #[allow(dead_code)]
    fn flush_tlb_page(addr: VirtAddr);

    /// Create a mapper for a newly allocated, zeroed root page table.
    ///
    /// The implementation receives an [`OwnedFrame`](crate::mm::OwnedFrame)
    /// that it must:
    ///
    /// 1. Zero.
    /// 2. Record its physical address for CR3 / TTBR0 use.
    /// 3. Return as concrete `Self::Mapper` instance.
    ///
    /// Ownership of `root` is transferred *into* the returned mapper so the
    /// mapper can track which frame to load into the hardware register.
    ///
    /// # Safety
    ///
    /// - The physical frame allocator must be initialised.
    /// - The HHDM must be initialised and accessible.
    /// - `root` must not be aliased anywhere else.
    #[allow(dead_code)]
    unsafe fn create_mapper(root: crate::mm::OwnedFrame) -> Self::Mapper;

    /// Wrap the *currently active* page tables in a mapper without allocating
    /// a new root frame.
    ///
    /// Used during early boot to get a manipulable handle to the bootloader's
    /// page tables before the kernel switches to its own.
    ///
    /// # Safety
    ///
    /// - A valid root page table must currently be loaded into the hardware register.
    /// - `root_phys` must be the physical address of that table.
    /// - The HHDM must be initialised.
    #[allow(dead_code)]
    unsafe fn mapper_from_active(root_phys: crate::mm::PhysAddr) -> Self::Mapper;

    /// Create a fresh page-table mapper for use in tests.
    #[cfg(feature = "kernel-tests")]
    fn new_test_mapper() -> impl crate::mm::paging::Mapper;
}

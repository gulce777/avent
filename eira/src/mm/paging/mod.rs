//! Architecture-agnostic virtual memory interface.
//!
//! Call sites always talk to `mm::paging::Mapper`, never to the concrete
//! backend.

use crate::arch::Arch;
use crate::mm::{Frame4K, OwnedFrame, PhysAddr, VirtAddr};

#[cfg(feature = "kernel-tests")]
pub mod tests;

/// Architecture-agnostic page protection flags.
///
/// These map to architecture-specific PTE bits inside each backend. Flags not
/// supported by a given architecture are silently ignored by that backend.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PageFlags(u8);

#[allow(non_upper_case_globals)]
impl PageFlags {
    /// Page may be read. Always implied when a mapping is present.
    pub const READ: Self = Self(1 << 0);
    /// Page may be written.
    pub const WRITE: Self = Self(1 << 1);
    /// Code may be fetched from this page.
    pub const EXECUTE: Self = Self(1 << 2);
    /// Mapping is accessible from user space (ring 3 / EL0).
    pub const USER: Self = Self(1 << 3);
    /// Disable caching, useful for MMIO regions.
    pub const UNCACHED: Self = Self(1 << 4);
    /// Global mapping. not invalidated on address-space switches.
    /// Appropriate for kernel mappings present in every address space.
    pub const GLOBAL: Self = Self(1 << 5);

    /// No flags set.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Typical kernel data mapping: readable + writable, no-execute, global.
    pub const fn kernel_data() -> Self {
        Self(Self::READ.0 | Self::WRITE.0 | Self::GLOBAL.0)
    }

    /// Typical kernel code mapping: readable + executable, no-write, global.
    pub const fn kernel_code() -> Self {
        Self(Self::READ.0 | Self::EXECUTE.0 | Self::GLOBAL.0)
    }

    /// Typical user data mapping: readable + writable, no-execute.
    pub const fn user_data() -> Self {
        Self(Self::READ.0 | Self::WRITE.0 | Self::USER.0)
    }

    /// Typical user code mapping: readable + executable.
    pub const fn user_code() -> Self {
        Self(Self::READ.0 | Self::EXECUTE.0 | Self::USER.0)
    }

    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn or(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[inline]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl core::ops::BitOr for PageFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.or(rhs)
    }
}

impl core::ops::BitOrAssign for PageFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// A pending TLB flush for a single virtual address.
///
/// Returned by [`Mapper::map`] and [`Mapper::unmap`]. The caller **must**
/// consume this by calling [`flush`](TlbFlush::flush) or [`ignore`](TlbFlush::ignore).
/// Dropping it without either will panic.
#[must_use = "TLB flush must be explicitly flushed or ignored"]
pub struct TlbFlush(VirtAddr);

impl TlbFlush {
    /// Construct a flush token for `addr`.
    ///
    /// Only [`Mapper`] implementations construct this type.
    pub(crate) fn new(addr: VirtAddr) -> Self {
        Self(addr)
    }

    /// This is a thin wrapper, the actual instruction is emitted by the arch
    /// backend via [`crate::arch::Platform::flush_tlb_page`].
    #[inline]
    pub fn flush(self) {
        let addr = self.0;
        core::mem::forget(self);

        // SAFETY: flushing a single page is always safe.
        unsafe { crate::arch::Platform::flush_tlb_page(addr) };
    }

    /// Acknowledge that the flush is intentionally deferred or not needed.
    ///
    /// Use this when you are about to switch address spaces anyway.
    #[inline]
    pub fn ignore(self) {
        core::mem::forget(self);
    }
}

#[cfg(debug_assertions)]
impl Drop for TlbFlush {
    fn drop(&mut self) {
        panic!(
            "TlbFlush for {:#x} dropped without being flushed or ignored",
            self.0.as_usize()
        );
    }
}

/// Issue a single-page TLB invalidation for `addr` on the current core.
///
/// # Safety
///
/// Must be called from a context where the relevant address space is active.
#[inline]
unsafe fn flush_tlb_page(addr: VirtAddr) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!(
            "invlpg [{addr}]",
            addr = in(reg) addr.as_usize(),
            options(nostack, preserves_flags),
        );
    }
}

/// Errors that [`Mapper::map`] can return.
#[derive(Debug, PartialEq, Eq)]
pub enum MapError {
    /// The virtual address is already mapped.
    AlreadyMapped,
    /// A page table frame could not be allocated.
    FrameAllocationFailed,
    /// The virtual address is not page-aligned.
    UnalignedAddress,
}

impl core::fmt::Display for MapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AlreadyMapped => write!(f, "virtual address is already mapped"),
            Self::FrameAllocationFailed => write!(f, "could not allocate a page table frame"),
            Self::UnalignedAddress => write!(f, "virtual address is not page-aligned"),
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum UnmapError {
    /// The virtual address was not mapped.
    NotMapped,
    /// The virtual address is not page-aligned.
    UnalignedAddress,
    /// The entry is a huge page; use the huge-page unmap path instead.
    HugePage,
}

impl core::fmt::Display for UnmapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotMapped => write!(f, "virtual address is not mapped"),
            Self::UnalignedAddress => write!(f, "virtual address is not page-aligned"),
            Self::HugePage => write!(f, "address is backed by a huge page"),
        }
    }
}

impl core::fmt::Debug for UnmapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

/// The interface all architecture backends must implement.
///
/// A `Mapper` owns or borrows one root page table (PML4 on x86_64) and provides
/// operations to inspect or mutate the mapping it describes.
///
/// # Frame allocator
///
/// Intermediate page table nodes (PDPT, PD, PT on x86_64) are allocated on
/// demand. The mapper calls back into the kernel's global frame allocator for
/// this. Callers do not need to supply frames for the tables themselves.
///
/// # TLB coherence
///
/// Every mutating method returns a [`TlbFlush`] token that **must** be
/// consumed by calling `.flush()` or `.ignore()`. Failing to flush after a
/// mapping change leaves stale entries in the TLB and causes silent
/// memory-safety violations.
pub trait Mapper {
    /// Map `virt` -> `frame` with the given protection flags.
    ///
    /// Creates intermediate page table nodes as needed.
    ///
    /// # Errors
    ///
    /// - [`MapError::AlreadyMapped`]: a mapping already exists for `virt`.
    /// - [`MapError::FrameAllocationFailed`]: ran out of physical memory
    ///   while allocating an intermediate table.
    /// - [`MapError::UnalignedAddress`]: `virt` is not page-aligned.
    fn map(
        &mut self,
        virt: VirtAddr,
        frame: OwnedFrame,
        flags: PageFlags,
    ) -> Result<TlbFlush, (OwnedFrame, MapError)>;

    /// Unmap the page at `virt`.
    ///
    /// Does **not** free the returned frame; the caller is responsible for
    /// deciding what to do with it (free it, reuse it, hand it to user space).
    ///
    /// # Errors
    ///
    /// - [`UnmapError::NotMapped`]: no mapping exists for `virt`.
    /// - [`UnmapError::UnalignedAddress`]: `virt` is not page-aligned.
    /// - [`UnmapError::HugePage`]: the address is backed by a huge page.
    fn unmap(&mut self, virt: VirtAddr) -> Result<(OwnedFrame, TlbFlush), UnmapError>;

    /// Translate `virt` to a physical address and its current flags.
    ///
    /// Returns `None` if `virt` is not mapped.
    fn translate(&self, virt: VirtAddr) -> Option<(PhysAddr, PageFlags)>;

    /// Update the flags on an existing mapping without changing the frame.
    ///
    /// # Errors
    ///
    /// - [`UnmapError::NotMapped`]: no mapping exists for `virt`.
    /// - [`UnmapError::UnalignedAddress`]: `virt` is not page-aligned.
    /// - [`UnmapError::HugePage`]: the address is backed by a huge page.
    fn remap(&mut self, virt: VirtAddr, new_flags: PageFlags) -> Result<TlbFlush, UnmapError>;
}

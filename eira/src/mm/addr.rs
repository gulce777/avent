//! Physical and virtual address types.
//!
//! Raw `usize` values are error-prone when used as addresses. This module provides
//! two distinct newtype wrappers.

use core::fmt;

/// The standard 4 KiB page size.
pub const PAGE_SIZE: usize = 4096;

/// A 64-bit physical memory address.
///
/// On both x86_64 and aarch64 with 4-level paging, only bits 0-51 are usable.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysAddr(usize);

impl PhysAddr {
    /// The largest representable physical address (2^52 - 1).
    pub const MAX: Self = Self((1usize << 52) - 1);

    /// Construct a `PhysAddr`, returning `None` if `addr` has bits set above
    /// bit 51 (i.e. the address is not representable in 52-bit physical space).
    #[inline]
    pub const fn new(addr: usize) -> Option<Self> {
        if addr <= Self::MAX.0 {
            Some(Self(addr))
        } else {
            None
        }
    }

    /// Construct a `PhysAddr` without validating the upper bits.
    ///
    /// # Safety
    ///
    /// `addr` must not have bits set above bit 51.
    #[inline]
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }

    /// Construct a `PhysAddr` from a `usize`, truncating any bits above bit 51.
    ///
    /// Prefer [`new`](Self::new) or [`new_unchecked`](Self::new_unchecked) when
    /// the address is known to be valid.
    #[inline]
    #[allow(dead_code)]
    pub const fn new_truncate(addr: usize) -> Self {
        Self(addr & Self::MAX.0)
    }

    /// Returns the address as a `usize`.
    #[inline]
    pub const fn as_usize(self) -> usize {
        self.0
    }

    /// Returns `true` if the address is aligned to `align` bytes.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two.
    #[inline]
    pub const fn is_aligned(self, align: usize) -> bool {
        assert!(align.is_power_of_two(), "align must be a power of two");
        self.0 & (align - 1) == 0
    }

    /// Aligns the address **down** to the nearest multiple of `align`.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two.
    #[inline]
    pub const fn align_down(self, align: usize) -> Self {
        assert!(align.is_power_of_two(), "align must be a power of two");
        Self(self.0 & !(align - 1))
    }

    /// Aligns the address **up** to the nearest multiple of `align`.
    ///
    /// Returns `None` if the result would overflow the 52-bit address space.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two.
    #[inline]
    pub const fn align_up(self, align: usize) -> Option<Self> {
        assert!(align.is_power_of_two(), "align must be a power of two");
        let mask = align - 1;
        match self.0.checked_add(mask) {
            Some(v) => Self::new(v & !mask),
            None => None,
        }
    }

    /// Returns the address as a raw const pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure the physical address is mapped in the current
    /// virtual address space (e.g. via the HHDM).
    #[inline]
    #[allow(dead_code)]
    pub const fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    /// Returns the address as a raw mutable pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure the physical address is mapped and writable in
    /// the current virtual address space.
    #[inline]
    #[allow(dead_code)]
    pub const fn as_mut_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }

    /// Add an offset to this address.
    ///
    /// Returns `None` on overflow or if the result exceeds 52 bits.
    #[inline]
    pub const fn checked_add(self, rhs: usize) -> Option<Self> {
        match self.0.checked_add(rhs) {
            Some(v) => Self::new(v),
            None => None,
        }
    }

    /// Subtract an offset from this address.
    ///
    /// Returns `None` on underflow.
    #[inline]
    #[allow(dead_code)]
    pub const fn checked_sub(self, rhs: usize) -> Option<Self> {
        match self.0.checked_sub(rhs) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }
}

impl fmt::Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhysAddr({:#018x})", self.0)
    }
}

impl fmt::Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#018x}", self.0)
    }
}

impl fmt::LowerHex for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

/// A 64-bit canonical virtual memory address.
///
/// On x86_64, bits 48-63 must be copies of bit 47 (sign-extension).
/// On aarch64 with 4-level paging (`T0SZ`/`T1SZ` = 16), bits 48-63 must
/// be all-zero (TTBR0, user) or all-one (TTBR1, kernel).
///
/// Canonicality is enforced on constructon. [`new`](Self::new) returns
/// `None` for non-canonical addresses, and [`new_canonical`](Self::new_canonical)
/// sign-extends the input automatically.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct VirtAddr(usize);

impl VirtAddr {
    /// Construct a canonical `VirtAddr`, returning `None` if `addr` is
    /// non-canonical (bits 48–63 are not a sign-extension of bit 47).
    #[inline]
    #[allow(dead_code)]
    pub const fn new(addr: usize) -> Option<Self> {
        if Self::is_canonical(addr) {
            Some(Self(addr))
        } else {
            None
        }
    }

    /// Construct a `VirtAddr` without canonicality validation.
    ///
    /// # Safety
    ///
    /// `addr` must be a canonical virtual address.
    #[inline]
    #[allow(dead_code)]
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }

    /// Sign-extend bit 47 to make `addr` canonical, then wrap in `VirtAddr`.
    #[inline]
    #[allow(dead_code)]
    pub const fn new_canonical(addr: usize) -> Self {
        Self(((addr << 16) as isize >> 16) as usize)
    }

    /// Returns `true` if `addr` is canonical.
    #[inline]
    const fn is_canonical(addr: usize) -> bool {
        let top = addr >> 47;
        top == 0 || top == (1 << 17) - 1
    }

    pub const fn is_canonical_addr(self) -> bool {
        Self::is_canonical(self.0)
    }

    /// Returns the address as a `usize`.
    #[inline]
    pub const fn as_usize(self) -> usize {
        self.0
    }

    /// Returns `true` if the address is aligned to `align` bytes.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two.
    #[inline]
    pub const fn is_aligned(self, align: usize) -> bool {
        assert!(align.is_power_of_two());
        self.0 & (align - 1) == 0
    }

    /// Aligns the address **down** to the nearest multiple of `align`.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two.
    #[inline]
    #[allow(dead_code)]
    pub const fn align_down(self, align: usize) -> Self {
        assert!(align.is_power_of_two());
        Self::new_canonical(self.0 & !(align - 1))
    }

    /// Aligns the address **up** to the nearest multiple of `align`.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two.
    #[inline]
    #[allow(dead_code)]
    pub const fn align_up(self, align: usize) -> Option<Self> {
        assert!(align.is_power_of_two());
        let mask = align - 1;
        match self.0.checked_add(mask) {
            Some(v) => Some(Self::new_canonical(v & !mask)),
            None => None,
        }
    }

    /// Returns the address as a raw const pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure this address is valid and mapped in the
    /// current address space.
    #[inline]
    #[allow(dead_code)]
    pub const fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    /// Returns the address as a raw mutable pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure this address is valid, mapped and writable
    /// in the current address space.
    #[inline]
    #[allow(dead_code)]
    pub const fn as_mut_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }
}

impl fmt::Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VirtAddr({:#018x})", self.0)
    }
}

impl fmt::Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#018x}", self.0)
    }
}

impl fmt::LowerHex for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

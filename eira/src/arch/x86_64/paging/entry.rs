//! x86_64 page table entry type and hardware flags.

use crate::mm::{Frame4K, PAGE_SIZE, PhysAddr, PhysFrame};

const PHYS_MASK: u64 = 0x000f_ffff_ffff_f000;

/// Raw hardware flags for an x86_64 page table entry.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct EntryFlags(u64);

#[allow(non_upper_case_globals)]
impl EntryFlags {
    pub const PRESENT: Self = Self(1 << 0);
    pub const WRITABLE: Self = Self(1 << 1);
    pub const USER: Self = Self(1 << 2);
    pub const WRITE_THROUGH: Self = Self(1 << 3);
    pub const CACHE_DISABLE: Self = Self(1 << 4);
    pub const ACCESSED: Self = Self(1 << 5);
    pub const DIRTY: Self = Self(1 << 6);
    pub const HUGE_PAGE: Self = Self(1 << 7);
    pub const GLOBAL: Self = Self(1 << 8);
    pub const NO_EXECUTE: Self = Self(1 << 63);

    pub const fn empty() -> Self {
        Self(0)
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
    #[inline]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

impl core::ops::BitOr for EntryFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.or(rhs)
    }
}

impl core::ops::BitOrAssign for EntryFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl core::fmt::Debug for EntryFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        macro_rules! flag {
            ($name:ident) => {
                if self.contains(Self::$name) {
                    write!(f, concat!(stringify!($name), " "))?;
                }
            };
        }
        flag!(PRESENT);
        flag!(WRITABLE);
        flag!(USER);
        flag!(WRITE_THROUGH);
        flag!(CACHE_DISABLE);
        flag!(ACCESSED);
        flag!(DIRTY);
        flag!(HUGE_PAGE);
        flag!(GLOBAL);
        flag!(NO_EXECUTE);
        Ok(())
    }
}

use crate::mm::paging::PageFlags;

/// Convert the arch-agnostic [`PageFlags`] into x86_64 hardware [`EntryFlags`].
///
/// `PRESENT` is always set; `NO_EXECUTE` is set when `EXECUTE` is absent.
pub(super) fn entry_flags_from_page_flags(pf: PageFlags) -> EntryFlags {
    let mut ef = EntryFlags::PRESENT;

    if pf.contains(PageFlags::WRITE) {
        ef |= EntryFlags::WRITABLE;
    }
    if pf.contains(PageFlags::USER) {
        ef |= EntryFlags::USER;
    }
    if pf.contains(PageFlags::UNCACHED) {
        ef |= EntryFlags::CACHE_DISABLE;
    }
    if pf.contains(PageFlags::GLOBAL) {
        ef |= EntryFlags::GLOBAL;
    }
    if !pf.contains(PageFlags::EXECUTE) {
        ef |= EntryFlags::NO_EXECUTE;
    }

    ef
}

/// Convert x86_64 hardware [`EntryFlags`] back to arch-agnostic [`PageFlags`].
pub(super) fn page_flags_from_entry_flags(ef: EntryFlags) -> PageFlags {
    let mut pf = PageFlags::READ; // present implies readable

    if ef.contains(EntryFlags::WRITABLE) {
        pf |= PageFlags::WRITE;
    }
    if ef.contains(EntryFlags::USER) {
        pf |= PageFlags::USER;
    }
    if ef.contains(EntryFlags::CACHE_DISABLE) {
        pf |= PageFlags::UNCACHED;
    }
    if ef.contains(EntryFlags::GLOBAL) {
        pf |= PageFlags::GLOBAL;
    }
    if !ef.contains(EntryFlags::NO_EXECUTE) {
        pf |= PageFlags::EXECUTE;
    }

    pf
}

/// A single 64-bit x86_64 page table entry.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    /// A zeroed, not-present entry.
    pub const EMPTY: Self = Self(0);

    /// Returns `true` if the [`PRESENT`](EntryFlags::PRESENT) flag is set.
    #[inline]
    pub fn is_present(self) -> bool {
        self.flags().contains(EntryFlags::PRESENT)
    }

    /// Returns `true` if the [`HUGE_PAGE`](EntryFlags::HUGE_PAGE) flag is set.
    ///
    /// At level 2 this means a 2 MiB page; at level 3 a 1 GiB page.
    #[inline]
    pub fn is_huge(self) -> bool {
        self.flags().contains(EntryFlags::HUGE_PAGE)
    }

    /// Extracts the raw hardware flags.
    #[inline]
    pub fn flags(self) -> EntryFlags {
        EntryFlags(self.0 & !PHYS_MASK)
    }

    /// Extracts the physical frame this entry points to.
    ///
    /// Returns `None` when the entry is not present.
    #[inline]
    pub fn frame(self) -> Option<Frame4K> {
        if !self.is_present() {
            return None;
        }
        let phys = unsafe { PhysAddr::new_unchecked((self.0 & PHYS_MASK) as usize) };
        PhysFrame::from_base(phys)
    }

    /// Construct an entry from a frame and flags.
    #[inline]
    pub fn new(frame: Frame4K, flags: EntryFlags) -> Self {
        let addr = frame.base().as_usize() as u64;
        Self((addr & PHYS_MASK) | flags.bits())
    }

    /// Zero this entry (marks it not-present).
    #[inline]
    pub fn clear(&mut self) {
        self.0 = 0;
    }

    /// Raw 64-bit value.
    #[inline]
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl core::fmt::Debug for PageTableEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PageTableEntry")
            .field("frame", &self.frame())
            .field("flags", &self.flags())
            .finish()
    }
}

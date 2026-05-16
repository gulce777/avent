//! [`PageTable`] and [`PageTableLevel`] for x86_64 4-level paging.
//!
//! Each table occupies exactly one 4 KiB frame and contains 512 entries.

use super::entry::PageTableEntry;

/// Number of entries in one page table.
pub const ENTRY_COUNT: usize = 512;

/// The four levels of the x86_64 page-table hierarchy.
///
/// Numbered top-down: 4 = PML4 (root), 1 = PT (leaf).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PageTableLevel {
    /// Page Table. Each present entry maps one 4 KiB frame.
    One = 1,
    /// Page Directory. Each entry points to a PT or is a 2 MiB huge leaf.
    Two = 2,
    /// Page Directory Pointer Table. Each entry points to a PD or is a 1 GiB huge leaf.
    Three = 3,
    /// PML4, pointed to by CR3.
    Four = 4,
}

impl PageTableLevel {
    /// The next level towards the leaf, or `None` at the leaf itself.
    #[inline]
    #[allow(dead_code)]
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Four => Some(Self::Three),
            Self::Three => Some(Self::Two),
            Self::Two => Some(Self::One),
            Self::One => None,
        }
    }

    /// `true` if this is the leaf (PT) level.
    #[inline]
    #[allow(dead_code)]
    pub const fn is_leaf(self) -> bool {
        matches!(self, Self::One)
    }

    /// Bit-shift to apply to a virtual address to extract this level's 9-bit index.
    ///
    /// | Level | Bits  | VA range  |
    /// |-------|-------|-----------|
    /// | 1     | 12–20 | PT index  |
    /// | 2     | 21–29 | PD index  |
    /// | 3     | 30–38 | PDPT index|
    /// | 4     | 39–47 | PML4 index|
    #[inline]
    pub const fn index_shift(self) -> u32 {
        12 + (self as u32 - 1) * 9
    }

    /// Extract this level's 9-bit table index from a virtual address.
    #[inline]
    pub fn index_of(self, virt: crate::mm::VirtAddr) -> usize {
        (virt.as_usize() >> self.index_shift()) & 0x1FF
    }
}

/// A 4 KiB page table holding 512 [`PageTableEntry`] values.
///
/// # Safety invariant
///
/// Every `PageTable` must reside at a page-aligned physical address. The mapper
/// is responsible for upholding this when constructing new tables.
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; ENTRY_COUNT],
}

// Compile-time size check — a page table must fit in one 4 KiB frame.
const _: () = assert!(
    core::mem::size_of::<PageTable>() == 4096,
    "PageTable must be exactly 4096 bytes"
);

impl PageTable {
    /// A zeroed (all-not-present) page table.
    pub const fn new() -> Self {
        Self {
            entries: [PageTableEntry::EMPTY; ENTRY_COUNT],
        }
    }

    /// Zero all entries.
    #[inline]
    pub fn zero(&mut self) {
        for e in &mut self.entries {
            e.clear();
        }
    }

    /// Returns a shared reference to the entry at `index`.
    ///
    /// # Panics
    ///
    /// Panics if `index >= 512`.
    #[inline]
    pub fn entry(&self, index: usize) -> &PageTableEntry {
        &self.entries[index]
    }

    /// Returns a mutable reference to the entry at `index`.
    ///
    /// # Panics
    ///
    /// Panics if `index >= 512`.
    #[inline]
    pub fn entry_mut(&mut self, index: usize) -> &mut PageTableEntry {
        &mut self.entries[index]
    }

    /// Iterate over all entries.
    #[inline]
    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = &PageTableEntry> {
        self.entries.iter()
    }
}

impl Default for PageTable {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for PageTable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let present_count = self.entries.iter().filter(|e| e.is_present()).count();
        f.debug_struct("PageTable")
            .field("present_entries", &present_count)
            .finish()
    }
}

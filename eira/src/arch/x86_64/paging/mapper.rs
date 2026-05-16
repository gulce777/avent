//! x86_64 implementation of [`mm::paging::Mapper`].
//!
//! [`PageTableMapper`] walks the 4-level page table hierarchy (PML4 -> PDPT ->
//! PD _> PT). It borrows the HHDM offset to convert physical addresses to
//! kernel virtual pointers without an extra allocator call.
//!
//! # HHDM usage
//!
//! The mapper never calls `PhysAddr::as_ptr` directly. All physical -> virtual
//! conversions go through [`phys_to_virt`] which adds the HHDM offset. This
//! mirrors how the bitmap allocator already works in `mm::init`.

use super::{
    entry::{EntryFlags, PageTableEntry, entry_flags_from_page_flags, page_flags_from_entry_flags},
    table::{PageTable, PageTableLevel},
};
use crate::mm::{
    Frame4K, OwnedFrame, PhysAddr, VirtAddr,
    paging::{MapError, Mapper, PageFlags, TlbFlush, UnmapError},
};

/// Convert a physical address to a kernel virtual pointer using the HHDM.
///
/// # Safety
///
/// `phys` must be a valid, mapped physical address. The HHDM must have been
/// initialised before calling this (i.e. after `mm::init::init()`).
#[inline]
#[allow(dead_code)]
unsafe fn phys_to_virt(phys: PhysAddr, hhdm: usize) -> *mut u8 {
    (phys.as_usize() + hhdm) as *mut u8
}

/// Obtain a mutable reference to the [`PageTable`] at physical frame `frame`.
///
/// # Safety
///
/// - The HHDM must be initialised.
/// - `frame` must contain a valid, properly aligned `PageTable`.
/// - No other live reference to the same frame may exist simultaneously.
#[inline]
#[allow(dead_code)]
unsafe fn table_at_frame(frame: Frame4K, hhdm: usize) -> &'static mut PageTable {
    let ptr = unsafe { phys_to_virt(frame.base(), hhdm) as *mut PageTable };
    // SAFETY: caller guarantees alignment and exclusive access.
    unsafe { &mut *ptr }
}

// ── PageTableMapper ───────────────────────────────────────────────────────────

/// An x86_64 page-table mapper tied to one root PML4 frame.
///
/// # Lifetime and ownership
///
/// `PageTableMapper` does *not* own the PML4 frame or any of the intermediate
/// frames it allocates. Ownership of those frames is tracked externally (e.g.
/// by the `AddressSpace` that owns this mapper). Dropping a mapper does not
/// free any frames.
pub struct PageTableMapper {
    /// Physical frame of the PML4 (CR3 value, page-aligned).
    pml4: Frame4K,
    /// Higher-half direct map offset — added to every physical address to get
    /// the kernel virtual address where that frame is accessible.
    hhdm_offset: usize,
}

impl PageTableMapper {
    /// Create a mapper for the PML4 rooted at `pml4`.
    ///
    /// # Safety
    ///
    /// - `pml4` must contain a valid, zeroed or previously populated PML4.
    /// - `hhdm_offset` must be the value returned by Limine's HHDM request.
    #[allow(dead_code)]
    pub unsafe fn new(pml4: Frame4K, hhdm_offset: usize) -> Self {
        Self { pml4, hhdm_offset }
    }

    /// Returns the PML4 frame (i.e. the value to load into CR3).
    #[inline]
    #[allow(dead_code)]
    pub fn pml4(&self) -> Frame4K {
        self.pml4
    }

    /// Walk the page table hierarchy for `virt`, returning the level-1 (PT)
    /// entry if the full walk succeeds.
    ///
    /// Returns `None` if any intermediate entry is not present, or if a huge
    /// page is encountered above level 1.
    #[allow(dead_code)]
    fn walk(&self, virt: VirtAddr) -> Option<&PageTableEntry> {
        let hhdm = self.hhdm_offset;

        // Level 4 -> 3 -> 2 -> 1
        let l4 = unsafe { table_at_frame(self.pml4, hhdm) };
        let l4e = l4.entry(PageTableLevel::Four.index_of(virt));
        if !l4e.is_present() {
            return None;
        }

        let l3 = unsafe { table_at_frame(l4e.frame()?, hhdm) };
        let l3e = l3.entry(PageTableLevel::Three.index_of(virt));
        if !l3e.is_present() || l3e.is_huge() {
            return None;
        }

        let l2 = unsafe { table_at_frame(l3e.frame()?, hhdm) };
        let l2e = l2.entry(PageTableLevel::Two.index_of(virt));
        if !l2e.is_present() || l2e.is_huge() {
            return None;
        }

        let l1 = unsafe { table_at_frame(l2e.frame()?, hhdm) };
        Some(l1.entry(PageTableLevel::One.index_of(virt)))
    }

    /// Same as [`walk`] but returns a mutable reference to the leaf entry.
    ///
    /// If any intermediate table does not exist and `alloc` is `true`, it is
    /// allocated from the global frame allocator and zeroed.
    #[allow(dead_code)]
    fn walk_or_create(
        &mut self,
        virt: VirtAddr,
        alloc: bool,
    ) -> Result<&mut PageTableEntry, MapError> {
        let hhdm = self.hhdm_offset;

        macro_rules! next_table {
            ($entry:expr, $level:expr) => {{
                if !$entry.is_present() {
                    if !alloc {
                        return Err(MapError::FrameAllocationFailed);
                    }
                    let frame = crate::mm::allocate();
                    let owned_base = frame.base();

                    // SAFETY: The frame was just allocated; no other reference exists.
                    unsafe {
                        let ptr = phys_to_virt(owned_base, hhdm) as *mut PageTable;
                        (*ptr).zero();
                    }

                    let raw_frame = unsafe { frame.into_inner() };

                    *$entry = PageTableEntry::new(
                        raw_frame,
                        EntryFlags::PRESENT | EntryFlags::WRITABLE | EntryFlags::USER,
                    );
                }
                if $entry.is_huge() {
                    return Err(MapError::AlreadyMapped);
                }
                let child_frame = $entry.frame().ok_or(MapError::FrameAllocationFailed)?;
                // SAFETY: intermediate table frames are always valid after being
                // installed, and we hold &mut self so no other mapper alias exists.
                unsafe { table_at_frame(child_frame, hhdm) }
            }};
        }

        let l4 = unsafe { table_at_frame(self.pml4, hhdm) };
        let l4e = l4.entry_mut(PageTableLevel::Four.index_of(virt));
        let l3 = next_table!(l4e, Three);
        let l3e = l3.entry_mut(PageTableLevel::Three.index_of(virt));
        let l2 = next_table!(l3e, Two);
        let l2e = l2.entry_mut(PageTableLevel::Two.index_of(virt));
        let l1 = next_table!(l2e, One);

        Ok(l1.entry_mut(PageTableLevel::One.index_of(virt)))
    }
}

impl Mapper for PageTableMapper {
    fn map(
        &mut self,
        virt: VirtAddr,
        frame: OwnedFrame,
        flags: PageFlags,
    ) -> Result<TlbFlush, (OwnedFrame, MapError)> {
        if !virt.is_aligned(crate::mm::PAGE_SIZE) {
            return Err((frame, MapError::UnalignedAddress));
        }

        if !virt.is_canonical_addr() {
            return Err((frame, MapError::UnalignedAddress));
        }

        let leaf = match self.walk_or_create(virt, true) {
            Ok(l) => l,
            Err(e) => return Err((frame, e)),
        };

        if leaf.is_present() {
            return Err((frame, MapError::AlreadyMapped));
        }

        let raw = unsafe { frame.into_inner() };
        *leaf = PageTableEntry::new(raw, entry_flags_from_page_flags(flags));

        Ok(TlbFlush::new(virt))
    }

    fn unmap(&mut self, virt: VirtAddr) -> Result<(OwnedFrame, TlbFlush), UnmapError> {
        if !virt.is_aligned(crate::mm::PAGE_SIZE) {
            return Err(UnmapError::UnalignedAddress);
        }

        let hhdm = self.hhdm_offset;

        let l4 = unsafe { table_at_frame(self.pml4, hhdm) };
        let l3_frame = {
            let e = l4.entry(PageTableLevel::Four.index_of(virt));
            if !e.is_present() {
                return Err(UnmapError::NotMapped);
            }
            e.frame().ok_or(UnmapError::NotMapped)?
        };

        let l2_frame = {
            let l3 = unsafe { table_at_frame(l3_frame, hhdm) };
            let e = l3.entry(PageTableLevel::Three.index_of(virt));
            if !e.is_present() {
                return Err(UnmapError::NotMapped);
            }
            if e.is_huge() {
                return Err(UnmapError::HugePage);
            }
            e.frame().ok_or(UnmapError::NotMapped)?
        };

        let l1_frame = {
            let l2 = unsafe { table_at_frame(l2_frame, hhdm) };
            let e = l2.entry(PageTableLevel::Two.index_of(virt));
            if !e.is_present() {
                return Err(UnmapError::NotMapped);
            }
            if e.is_huge() {
                return Err(UnmapError::HugePage);
            }
            e.frame().ok_or(UnmapError::NotMapped)?
        };

        let leaf_frame = {
            let l1 = unsafe { table_at_frame(l1_frame, hhdm) };
            let e = l1.entry_mut(PageTableLevel::One.index_of(virt));
            if !e.is_present() {
                return Err(UnmapError::NotMapped);
            }
            let frame = e.frame().ok_or(UnmapError::NotMapped)?;
            e.clear();
            frame
        };

        Ok((OwnedFrame::new(leaf_frame), TlbFlush::new(virt)))
    }

    fn translate(&self, virt: VirtAddr) -> Option<(PhysAddr, PageFlags)> {
        let leaf = self.walk(virt)?;
        if !leaf.is_present() {
            return None;
        }
        let frame = leaf.frame()?;
        let page_offset = virt.as_usize() & (crate::mm::PAGE_SIZE - 1);
        let phys = frame.base().checked_add(page_offset)?;
        let flags = page_flags_from_entry_flags(leaf.flags());
        Some((phys, flags))
    }

    fn remap(&mut self, virt: VirtAddr, new_flags: PageFlags) -> Result<TlbFlush, UnmapError> {
        if !virt.is_aligned(crate::mm::PAGE_SIZE) {
            return Err(UnmapError::UnalignedAddress);
        }

        let leaf = self
            .walk_or_create(virt, false)
            .map_err(|_| UnmapError::NotMapped)?;

        if !leaf.is_present() {
            return Err(UnmapError::NotMapped);
        }
        if leaf.is_huge() {
            return Err(UnmapError::HugePage);
        }

        let frame = leaf.frame().ok_or(UnmapError::NotMapped)?;
        *leaf = PageTableEntry::new(frame, entry_flags_from_page_flags(new_flags));

        Ok(TlbFlush::new(virt))
    }
}

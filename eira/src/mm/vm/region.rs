//! Virtual address region allocator.
//!
//! [`RegionAllocator`] manages a virtual address space by tracking which
//! sub-ranges are free. It is intentionally allocation-free. The free list is
//! a fixed-capacity sorted array of [`Region`] slots embedded directly in the
//! struct, so it can be constructed and used before the kernel heap exists.
//!
//! # Invariants
//!
//! At all times the internal slot array satisfies:
//!
//! 1. Slots `[0, len)` are initialised and valid, slots `[len, CAPACITY)` are never read.
//! 2. Slots are sorted in ascending order of `base` address.
//! 3. No two slots overlap or are directly adjacent. Adjacent free regions are always merged
//!    immediately on insertion.

use crate::mm::{PAGE_SIZE, VirtAddr};

/// Maximum number of simultaneously tracked free regions.
pub const CAPACITY: usize = 64;

/// A contiguous, page-aligned range of virtual addresses `[base, base + size)`.
///
/// Both `base` and `size` are always multiples of [`PAGE_SIZE`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Region {
    /// Inclusive start address of the region.
    pub base: VirtAddr,
    /// Byte length. Always a non-zero multiple of [`PAGE_SIZE`].
    pub size: usize,
}

impl Region {
    /// Constructs a new region.
    ///
    /// # Panics
    ///
    /// - `size` is zero.
    /// - `base` is not page-aligned.
    /// - `size` is not a multiple of [`PAGE_SIZE`].
    #[inline]
    pub fn new(base: VirtAddr, size: usize) -> Self {
        assert!(size > 0, "region size must be non-zero");
        assert!(
            base.is_aligned(PAGE_SIZE),
            "region base {base:?} is not page-aligned",
        );
        assert!(
            size % PAGE_SIZE == 0,
            "region size {size:#x} is not a multiple of PAGE_SIZE",
        );
        Self { base, size }
    }

    /// The exclusive end address of this region (`base + size`).
    ///
    /// Returns `None` on arithmetic overflow. Callers should treat overflow as
    /// a bug in the virtual memory layout constants.
    #[inline]
    pub fn end(self) -> Option<VirtAddr> {
        // We skip `VirtAddr::new` canonicality enforcement here
        // because an end address at the very top of the canonical range
        // would fail that check even though the *range* itself is valid.
        // Raw arithmetic suffices for the comparisons we perform.
        let raw = self.base.as_usize().checked_add(self.size)?;
        Some(unsafe { VirtAddr::new_unchecked(raw) })
    }

    /// Returns `true` if this region is directly adjacent to `other` from the
    /// left (i.e. `self.end() == other.base`).
    #[inline]
    fn is_left_adjacent_to(self, other: Self) -> bool {
        self.end()
            .map(|e| e.as_usize() == other.base.as_usize())
            .unwrap_or(false)
    }
}

/// Errors returned by [`RegionAllocator`] operations.
#[derive(Debug, PartialEq, Eq)]
pub enum RegionError {
    /// No contiguous free region of the requested size exists.
    OutOfVirtualMemory,
    /// The slot array is full; a new free region cannot be recorded.
    ///
    /// This indicates either a bug (more `free` calls than `alloc` calls) or
    /// extreme address-space fragmentation.
    TrackerFull,
    /// The given address or size is not page-aligned.
    UnalignedAddress,
}

impl core::fmt::Display for RegionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutOfVirtualMemory => write!(f, "out of virtual address space"),
            Self::TrackerFull => write!(f, "region tracker slot array is full"),
            Self::UnalignedAddress => write!(f, "address or size is not page-aligned"),
        }
    }
}

/// A heap-free virtual address region allocator.
///
/// All state is stored in a fixed-capacity sorted array of [`Region`] slots
/// embedded directlyi in the struct. No dynamic allocation is required.
///
/// All operations are O(n) in the number of tracked free regions, bounded by
/// [`CAPACITY`]. For a kernel managing O(10-100) mappings this is optimal.
pub struct RegionAllocator {
    /// Sorted, non-overlapping, non-adjacent free regions.
    ///
    /// Only indices `[0, len)` contain valid data. The rest are uninitialised
    /// logically.
    slots: [Region; CAPACITY],
    /// Number of valid entries in `slots`.
    len: usize,
}

impl RegionAllocator {
    /// Construct an empty allocator with no tracked regions.
    ///
    /// Callers must subsequently call [`add_region`](Self::add_region) to
    /// register usable virtual address ranges before any allocation can
    /// succeed.
    pub const fn new() -> Self {
        // `Region::new` is not callable in a `const` context (panics are not
        // const-stable). We initialise with a harmless sentinel, slots beyond
        // `self.len` are never read by any safe method.
        const SENTINEL: Region = Region {
            base: unsafe { VirtAddr::new_unchecked(0) },
            size: PAGE_SIZE,
        };
        Self {
            slots: [SENTINEL; CAPACITY],
            len: 0,
        }
    }

    /// Register `region` as free virtual address space.
    ///
    /// The region is inserted in sorted order and immediately merged with any
    /// directly adjacent existing free regions to maintain the no-adjacency
    /// invariant.
    ///
    /// # Errors
    ///
    /// - [`RegionError::UnalignedAddress`] if `region` is not page-aligned.
    /// - [`RegionError::TrackerFull`] if the slot array is full after merging.
    pub fn add_region(&mut self, region: Region) -> Result<(), RegionError> {
        if !region.base.is_aligned(PAGE_SIZE) || region.size % PAGE_SIZE != 0 {
            return Err(RegionError::UnalignedAddress);
        }

        let pos =
            self.slots[..self.len].partition_point(|s| s.base.as_usize() < region.base.as_usize());

        self.insert_at(pos, region)?;
        self.try_merge_at(pos);

        Ok(())
    }

    /// Allocate a virtual region of `size` bytes with at least `align`-byte
    /// alignment.
    ///
    /// Uses a *first-fit* strategy: scans free regions from low to high and
    /// returns the first one that fits after alignment padding. Leftover
    /// fragments before and after the aligned sub-range are returned to the
    /// free list automatically.
    ///
    /// # Parameters
    ///
    /// - `size`  — byte count; must be a non-zero multiple of [`PAGE_SIZE`].
    /// - `align` — required alignment of the returned base address; must be a
    ///   power of two and a multiple of [`PAGE_SIZE`].
    ///
    /// # Errors
    ///
    /// - [`RegionError::UnalignedAddress`] if `size` or `align` violates the
    ///   constraints above.
    /// - [`RegionError::OutOfVirtualMemory`] if no suitable free region exists.
    pub fn alloc(&mut self, size: usize, align: usize) -> Result<Region, RegionError> {
        if size == 0 || size % PAGE_SIZE != 0 {
            return Err(RegionError::UnalignedAddress);
        }

        if !align.is_power_of_two() || align % PAGE_SIZE != 0 {
            return Err(RegionError::UnalignedAddress);
        }

        let found = self.slots[..self.len]
            .iter()
            .enumerate()
            .find_map(|(i, slot)| {
                let base_raw = slot.base.as_usize();

                let aligned_raw = base_raw.checked_add(align - 1)? & !(align - 1);
                let padding = aligned_raw - base_raw;
                let total_needed = padding.checked_add(size)?;
                if slot.size >= total_needed {
                    let aligned = unsafe { VirtAddr::new_unchecked(aligned_raw) };
                    Some((i, aligned, padding))
                } else {
                    None
                }
            });

        let (slot_idx, aligned_base, padding) = found.ok_or(RegionError::OutOfVirtualMemory)?;

        let slot = self.remove_at(slot_idx);

        if padding > 0 {
            let head = Region::new(slot.base, padding);
            self.insert_at(slot_idx, head)
                .expect("slot available after removal");
        }

        let tail_size = slot.size - padding - size;
        if tail_size > 0 {
            let tail_base_raw = aligned_base.as_usize() + size;
            let tail_base = unsafe { VirtAddr::new_unchecked(tail_base_raw) };
            let tail = Region::new(tail_base, tail_size);
            let tail_pos = slot_idx + usize::from(padding > 0);
            self.insert_at(tail_pos, tail)
                .expect("slot available after removal");
        }

        Ok(Region::new(aligned_base, size))
    }

    /// Return a previously allocated region to the free list.
    ///
    /// Adjacent free regions are merged automatically to prevent fragmentation.
    ///
    /// # Errors
    ///
    /// - [`RegionError::TrackerFull`] if no slot is available.
    /// - [`RegionError::UnalignedAddress`] if `region` is not page-aligned.
    #[inline]
    pub fn free(&mut self, region: Region) -> Result<(), RegionError> {
        self.add_region(region)
    }

    /// Total free bytes across all tracked regions.
    pub fn free_bytes(&self) -> usize {
        self.slots[..self.len].iter().map(|s| s.size).sum()
    }

    /// Number of disjoint free regions currently tracked.
    #[inline]
    #[allow(dead_code)]
    pub fn free_region_count(&self) -> usize {
        self.len
    }

    /// Insert `region` at index `pos`, shifting `slots[pos..]` one step right.
    ///
    /// # Errors
    ///
    /// Returns [`RegionError::TrackerFull`] if `self.len == CAPACITY`.
    fn insert_at(&mut self, pos: usize, region: Region) -> Result<(), RegionError> {
        if self.len >= CAPACITY {
            return Err(RegionError::TrackerFull);
        }
        self.slots.copy_within(pos..self.len, pos + 1);
        self.slots[pos] = region;
        self.len += 1;
        Ok(())
    }

    /// Remove and return the slot at index `pos`, shifting `slots[pos+1..]`
    /// one step left.
    ///
    /// # Panics
    ///
    /// Panics if `pos >= self.len`.
    fn remove_at(&mut self, pos: usize) -> Region {
        assert!(pos < self.len, "remove_at: index {pos} out of bounds");
        let region = self.slots[pos];
        self.slots.copy_within(pos + 1..self.len, pos);
        self.len -= 1;
        region
    }

    /// Attempt to merge the slot at `pos` with its immediate neighbours.
    ///
    /// Checks right-neighbour first (merging it does not change `pos`), then
    /// left-neighbour.
    fn try_merge_at(&mut self, pos: usize) {
        // Merge with right neighbour.
        if pos + 1 < self.len {
            let left = self.slots[pos];
            let right = self.slots[pos + 1];
            if left.is_left_adjacent_to(right) {
                self.slots[pos] = Region::new(left.base, left.size + right.size);
                self.remove_at(pos + 1);
            }
        }
        if pos > 0 {
            let left = self.slots[pos - 1];
            let right = self.slots[pos];
            if left.is_left_adjacent_to(right) {
                self.slots[pos - 1] = Region::new(left.base, left.size + right.size);
                self.remove_at(pos);
            }
        }
    }
}

impl Default for RegionAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for RegionAllocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list()
            .entries(self.slots[..self.len].iter())
            .finish()
    }
}

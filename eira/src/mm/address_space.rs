//! Virtual address space management.
//!
//! An [`AddressSpace<M>`] is the single authoritative owner of one root page
//! table and the virtual address ranges carved out of it. The type paramter
//! `M` is the architecture-specific [`Mapper`] implementation, all call sites
//! use the [`KernelAddressSpace`] alias so the parameter is never written out
//! explicitly.

use crate::arch::{Arch, Platform, PlatformMapper};
use crate::mm::init::FRAME_ALLOCATOR;
use crate::mm::paging::{MapError, Mapper, PageFlags, TlbFlush, UnmapError};
use crate::mm::vm::region::{Region, RegionAllocator, RegionError};
use crate::mm::{self, FrameAllocator, OwnedFrame, PAGE_SIZE, PhysAddr, VirtAddr};

#[allow(dead_code)]
pub type KernelAddressSpace = AddressSpace<PlatformMapper>;

/// Parameters for a single virtual-to-physical mapping operation.
///
/// Passed to [`AddressSpace::map`] to bundle all arguments and avoid long
/// parameter lists.
///
/// # Ownership
///
/// On success, [`frame`](Self::frame) is consumed by the page table hierarchy.
/// On failure, it is returned to the caller inside the error value so it can
/// be freed without leaking.
pub struct MapRequest {
    /// The virtual address to map. Must be page-aligned.
    pub virt: VirtAddr,
    /// The physical frame to back the mapping.
    pub frame: OwnedFrame,
    /// Page protection flags.
    pub flags: PageFlags,
}

/// Errors returned by [`AddressSpace`] operations.
#[derive(Debug, PartialEq, Eq)]
pub enum AddressSpaceError {
    /// An error from the virtual region allocator.
    Region(RegionError),
    /// An error from the page table mapper (map path).
    Map(MapError),
    /// An error from the page table mapper (unmap / remap path).
    Unmap(UnmapError),
    /// The physical frame allocator is exhausted.
    OutOfPhysicalMemory,
}

impl From<RegionError> for AddressSpaceError {
    fn from(e: RegionError) -> Self {
        Self::Region(e)
    }
}

impl From<MapError> for AddressSpaceError {
    fn from(e: MapError) -> Self {
        Self::Map(e)
    }
}

impl From<UnmapError> for AddressSpaceError {
    fn from(e: UnmapError) -> Self {
        Self::Unmap(e)
    }
}

impl core::fmt::Display for AddressSpaceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Region(e) => write!(f, "virtual region error: {e}"),
            Self::Map(e) => write!(f, "page table map error: {e}"),
            Self::Unmap(e) => write!(f, "page table unmap error: {e}"),
            Self::OutOfPhysicalMemory => write!(f, "out of physical memory"),
        }
    }
}

/// Which virtual address sub-region to allocate from.
///
/// Passed to [`AddressSpace::alloc_and_map`], [`AddressSpace::alloc_virt`],
/// and related helpers.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AllocKind {
    /// Allocate from the kernel heap virtual range.
    ///
    /// Use this for slab / buddy backing pages that will be managed by the
    /// kernel heap allocator.
    #[allow(dead_code)]
    Heap,

    /// Allocate from the general kernel VM virtual range.
    ///
    /// Use this for vmalloc-style mappings: per-CPU stacks, MMIO windows,
    /// framebuffers, and other objects that need a contiguous VA range but may
    /// span discontiguous physical frames.
    #[allow(dead_code)]
    KernelVm,
}

/// A virtual address space.
///
/// Owns the root page table frame, the architecture-specific mapper that
/// walks and mutates it, and two virtual region allocators that track which
/// VA ranges are available for new mappings.
pub struct AddressSpace<M: Mapper> {
    /// Owned root page table frame.
    #[allow(dead_code)]
    root: Option<OwnedFrame>,

    /// Architecture-specific page table walker / mutator.
    ///
    /// Contains a *copy* of the root frame's physical address (for CR3 /
    /// TTBR0 arithmetic). Actual ownership of the frame is in `self.root`.
    mapper: M,

    /// Virtual region allocator for the kernel heap VA range.
    heap_va: RegionAllocator,

    /// Virtual region allocator for the general kernel VM VA range.
    kernel_vm_va: RegionAllocator,
}

impl<M: Mapper> AddressSpace<M> {
    /// Create a new, empty address space backed by a freshly allocated and
    /// zeroed root page table frame.
    #[allow(dead_code)]
    pub fn new_kernel() -> Self
    where
        M: From<PlatformMapper>,
    {
        let frame = mm::allocate();

        let mapper: PlatformMapper = unsafe { Platform::create_mapper(frame) };

        Self {
            root: None,
            mapper: M::from(mapper),
            heap_va: crate::mm::vm::heap_region_allocator(),
            kernel_vm_va: crate::mm::vm::kernel_vm_region_allocator(),
        }
    }

    /// Wrap the *currently active* page tables in an `AddressSpace` without
    /// allocating a new root frame.
    ///
    /// Use this during early boot to obtain a manipulable handle to the
    /// bootloader's page tables before switching to kernel-owned ones.
    ///
    /// The returned `AddressSpace` does **not** own the root frame
    /// (`self.root == None`). The frame must remain valid for the entire
    /// lifetime of the returned value.
    ///
    /// # Safety
    ///
    /// - A valid root page table must currently be loaded in the hardware
    ///   register (CR3 on x86_64, TTBR0_EL1 on aarch64).
    /// - `root_phys` must be the physical address of that table.
    /// - The HHDM must be initialised.
    #[allow(dead_code)]
    pub unsafe fn from_active(root_phys: crate::mm::PhysAddr) -> Self
    where
        M: From<PlatformMapper>,
    {
        // SAFETY: forwarded from caller.
        let mapper: PlatformMapper = unsafe { Platform::mapper_from_active(root_phys) };

        Self {
            root: None,
            mapper: M::from(mapper),
            heap_va: crate::mm::vm::heap_region_allocator(),
            kernel_vm_va: crate::mm::vm::kernel_vm_region_allocator(),
        }
    }

    /// Construct an `AddressSpace` from a pre-built mapper.
    ///
    /// This is a low-level escape hatch intended for tests that supply a mock
    /// mapper. Production code should use [`new_kernel`](Self::new_kernel) or
    /// [`from_active`](Self::from_active).
    #[cfg(feature = "kernel-tests")]
    pub fn from_mapper(mapper: M) -> Self {
        Self {
            root: None,
            mapper,
            heap_va: crate::mm::vm::heap_region_allocator(),
            kernel_vm_va: crate::mm::vm::kernel_vm_region_allocator(),
        }
    }

    /// Map a specific virtual address to a specific physical frame.
    ///
    /// This is the low-level primitive. The virtual address must either already
    /// be reserved in the appropriate region allocator, or lie outside the
    /// managed regions (e.g. a fixed kernel symbol address).
    ///
    /// On success, ownership of `req.frame` is transferred to the page table
    /// hierarchy. On failure, the frame is returned to the caller inside the
    /// error tuple so it can be freed without leaking.
    ///
    /// # Returns
    ///
    /// A [`TlbFlush`] token that **must** be consumed by calling `.flush()`
    /// or `.ignore()`.
    ///
    /// # Errors
    ///
    /// [`AddressSpaceError::Map`] wrapping the underlying [`MapError`].
    pub fn map(&mut self, req: MapRequest) -> Result<TlbFlush, (OwnedFrame, AddressSpaceError)> {
        self.mapper
            .map(req.virt, req.frame, req.flags)
            .map_err(|(frame, e)| (frame, AddressSpaceError::Map(e)))
    }

    /// Unmap the page at `virt` and return the backing physical frame.
    ///
    /// The caller is responsible for freeing (or reusing) the returned frame.
    ///
    /// # Returns
    ///
    /// `(frame, flush)`: the reclaimed frame and the required TLB flush.
    ///
    /// # Errors
    ///
    /// [`AddressSpaceError::Unmap`] wrapping the underlying [`UnmapError`].
    #[allow(dead_code)]
    pub fn unmap(&mut self, virt: VirtAddr) -> Result<(OwnedFrame, TlbFlush), AddressSpaceError> {
        self.mapper.unmap(virt).map_err(AddressSpaceError::Unmap)
    }

    /// Update the protection flags on an existing mapping without moving the
    /// backing frame.
    ///
    /// # Returns
    ///
    /// A [`TlbFlush`] token.
    ///
    /// # Errors
    ///
    /// [`AddressSpaceError::Unmap`] wrapping the underlying [`UnmapError`].
    #[allow(dead_code)]
    pub fn remap(
        &mut self,
        virt: VirtAddr,
        new_flags: PageFlags,
    ) -> Result<TlbFlush, AddressSpaceError> {
        self.mapper
            .remap(virt, new_flags)
            .map_err(AddressSpaceError::Unmap)
    }

    /// Translate `virt` to a physical address and its current protection flags.
    ///
    /// Returns `None` if `virt` is not mapped.
    #[inline]
    #[allow(dead_code)]
    pub fn translate(&self, virt: VirtAddr) -> Option<(PhysAddr, PageFlags)> {
        self.mapper.translate(virt)
    }

    /// Allocate a virtual region and back every page with a fresh physical frame.
    #[allow(dead_code)]
    pub fn alloc_and_map(
        &mut self,
        size: usize,
        align: usize,
        flags: PageFlags,
        kind: AllocKind,
    ) -> Result<Region, AddressSpaceError> {
        let region = self.alloc_virt(size, align, kind)?;
        let base_raw = region.base.as_usize();
        let num_pages = size / PAGE_SIZE;

        for i in 0..num_pages {
            let virt = unsafe { VirtAddr::new_unchecked(base_raw + i * PAGE_SIZE) };

            let frame = match FRAME_ALLOCATOR.lock().as_mut().and_then(|a| a.allocate()) {
                Some(f) => f,
                None => {
                    // Roll back the pages already mapped, then free the VA.
                    self.rollback_range(region.base, i);
                    self.free_virt(region, kind)
                        .expect("VA region free during rollback cannot fail");
                    return Err(AddressSpaceError::OutOfPhysicalMemory);
                }
            };

            let req = MapRequest { virt, frame, flags };

            match self.map(req) {
                Ok(flush) => flush.ignore(),
                Err((frame, e)) => {
                    FRAME_ALLOCATOR
                        .lock()
                        .as_mut()
                        .expect("frame allocator must be initialised")
                        .deallocate_owned(frame);
                    self.rollback_range(region.base, i);
                    self.free_virt(region, kind)
                        .expect("VA region free during rollback cannot fail");
                    return Err(e);
                }
            }
        }

        self.flush_range(region.base, num_pages);

        Ok(region)
    }

    /// Unmap every page in `region`, free their backing frames, and return the
    /// VA range to the appropriate pool.
    ///
    /// TLB invalidations are batched after all unmaps.
    ///
    /// # Errors
    ///
    /// Returns the first [`AddressSpaceError::Unmap`] encountered, but
    /// continues unmapping remaining pages and always frees the VA region.
    #[allow(dead_code)]
    pub fn unmap_and_dealloc(
        &mut self,
        region: Region,
        kind: AllocKind,
    ) -> Result<(), AddressSpaceError> {
        let num_pages = region.size / PAGE_SIZE;
        let base_raw = region.base.as_usize();
        let mut first_err: Option<AddressSpaceError> = None;

        for i in 0..num_pages {
            let virt = unsafe { VirtAddr::new_unchecked(base_raw + i * PAGE_SIZE) };
            match self.mapper.unmap(virt) {
                Ok((frame, flush)) => {
                    flush.ignore();
                    FRAME_ALLOCATOR
                        .lock()
                        .as_mut()
                        .expect("frame allocator must be initialised")
                        .deallocate_owned(frame);
                }
                Err(e) if first_err.is_none() => {
                    first_err = Some(AddressSpaceError::Unmap(e));
                }
                Err(_) => {}
            }
        }

        self.free_virt(region, kind)
            .expect("VA region free cannot fail for a previously-allocated region");

        self.flush_range(region.base, num_pages);

        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Map a caller-supplied iterator of physical frames into a freshly
    /// allocated virtual region.
    ///
    /// Unlike [`alloc_and_map`](Self::alloc_and_map), this does *not* allocate
    /// physical frames, the caller provides them. Useful for MMIO regions,
    /// framebuffers, or any memory with a predetermined physical address.
    ///
    /// Ownership of every frame yielded by `frames` is transferred to the page
    /// tables on success.
    ///
    /// # Errors
    ///
    /// See [`alloc_and_map`](Self::alloc_and_map).
    #[allow(dead_code)]
    pub fn map_physical_range(
        &mut self,
        frames: impl Iterator<Item = OwnedFrame>,
        align: usize,
        flags: PageFlags,
        kind: AllocKind,
    ) -> Result<Region, AddressSpaceError> {
        // Collect eagerly so we know the count before reserving VA space.
        // In a no-alloc environment the caller must pass an `ExactSizeIterator`
        // and we could avoid the collection, but correctness is the priority
        // here; optimise when the heap exists.
        //
        // For now we iterate once to count, then re-use the frames.
        // Since we don't have a heap, we use a fixed-size array, the maximum
        // sensible MMIO mapping is bounded by the VM region size.
        //
        // A simpler approach: require `ExactSizeIterator`.
        let mut frame_buf: [core::mem::MaybeUninit<OwnedFrame>; 512] =
            unsafe { core::mem::MaybeUninit::uninit().assume_init() };
        let mut count = 0usize;

        for frame in frames {
            assert!(count < 512, "map_physical_range: too many frames (max 512)");
            frame_buf[count].write(frame);
            count += 1;
        }

        let size = count
            .checked_mul(PAGE_SIZE)
            .expect("frame count overflows address space");

        let region = self.alloc_virt(size, align, kind)?;
        let base_raw = region.base.as_usize();

        for i in 0..count {
            let virt = unsafe { VirtAddr::new_unchecked(base_raw + i * PAGE_SIZE) };
            // SAFETY: we initialised indices [0, count).
            let frame = unsafe { frame_buf[i].assume_init_read() };

            let req = MapRequest { virt, frame, flags };

            match self.map(req) {
                Ok(flush) => flush.ignore(),
                Err((frame, e)) => {
                    FRAME_ALLOCATOR
                        .lock()
                        .as_mut()
                        .expect("frame allocator must be initialised")
                        .deallocate_owned(frame);
                    self.rollback_range(region.base, i);
                    self.free_virt(region, kind)
                        .expect("VA region free during rollback cannot fail");
                    return Err(e);
                }
            }
        }

        self.flush_range(region.base, count);
        Ok(region)
    }

    /// Reserve a `size`-byte VA range aligned to `align` bytes from the pool
    /// selected by `kind`.
    ///
    /// Does not create any page table mappings.
    ///
    /// # Errors
    ///
    /// [`AddressSpaceError::Region`] if the pool is exhausted or the
    /// alignment constraints are invalid.
    pub fn alloc_virt(
        &mut self,
        size: usize,
        align: usize,
        kind: AllocKind,
    ) -> Result<Region, AddressSpaceError> {
        self.region_allocator_for(kind)
            .alloc(size, align)
            .map_err(AddressSpaceError::Region)
    }

    /// Return a previously reserved VA range to its pool.
    ///
    /// Does not touch any page table entries.
    ///
    /// # Errors
    ///
    /// [`AddressSpaceError::Region`] if the region is not page-aligned or the
    /// tracker is full.
    pub fn free_virt(&mut self, region: Region, kind: AllocKind) -> Result<(), AddressSpaceError> {
        self.region_allocator_for(kind)
            .free(region)
            .map_err(AddressSpaceError::Region)
    }

    #[inline]
    #[allow(dead_code)]
    pub fn heap_free_bytes(&self) -> usize {
        self.heap_va.free_bytes()
    }

    #[inline]
    #[allow(dead_code)]
    pub fn kernel_vm_free_bytes(&self) -> usize {
        self.kernel_vm_va.free_bytes()
    }

    /// Return a mutable reference to the region allocator for `kind`.
    #[inline]
    fn region_allocator_for(&mut self, kind: AllocKind) -> &mut RegionAllocator {
        match kind {
            AllocKind::Heap => &mut self.heap_va,
            AllocKind::KernelVm => &mut self.kernel_vm_va,
        }
    }

    /// Invalidate TLB entries for `count` consecutive pages starting at `base`.
    ///
    /// Issues one `flush_tlb_page` per page via the [`Platform`] abstraction,
    /// which dispatches to the correct architecture instruction (`invlpg` /
    /// `tlbi vaae1is`).
    #[inline]
    fn flush_range(&self, base: VirtAddr, count: usize) {
        let base_raw = base.as_usize();
        for i in 0..count {
            let virt = unsafe { VirtAddr::new_unchecked(base_raw + i * PAGE_SIZE) };
            Platform::flush_tlb_page(virt);
        }
    }

    /// Unmap the first `count` pages starting at `base` and free their frames.
    ///
    /// Used exclusively for rollback inside [`alloc_and_map`](Self::alloc_and_map)
    /// and [`map_physical_range`](Self::map_physical_range). Panics on any
    /// unmap error because we are rolling back pages we just successfully
    /// mapped, a failure here is always a programming error.
    fn rollback_range(&mut self, base: VirtAddr, count: usize) {
        let base_raw = base.as_usize();
        for i in 0..count {
            let virt = unsafe { VirtAddr::new_unchecked(base_raw + i * PAGE_SIZE) };
            let (frame, flush) = self
                .mapper
                .unmap(virt)
                .expect("rollback unmap must succeed for a page we just mapped");
            flush.ignore(); // caller issues a shootdown or machine is reset
            FRAME_ALLOCATOR
                .lock()
                .as_mut()
                .expect("frame allocator must be initialised")
                .deallocate_owned(frame);
        }
    }
}

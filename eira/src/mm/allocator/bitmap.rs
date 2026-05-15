//! Bitmap + free list hybrid physical frame allocator.
//!
//! # Algorithm
//!
//! Two data structures work in tandem:
//!
//! - **Bitmap:** one bit per frame. `0 = free, `1` = allocated. Provides O(1) `is_free`
//!   queries and O(1) mark/unmark operations.
//! - **Free list:** a singly-linked list threaded through the free frames themselves (the first 8 bytes
//!   of each free frame store the next pointer). Provides O(1) allocation and deallocations with zero additional
//!   memory overhead.
//!
//! # Memory layout
//!
//! The bitmap is stored in a caller-provided byte slice. At minimum you need `ceil(frame_count / 8)`
//! bytes. In practice, the kernel reserves a region of the early boot memory for the bitmap before the
//! heap is available.
//!
//! # Safety invariants
//!
//! - The HHDM offset must be provided at construction time so the allocator can convert physical addresses
//!   to virtual addresses when threading the free list through frame memory.
//! - The bitmap slice must outlive the allocator.
//! - The memory ranges added via [`add_region`](BitmapAllocator::add_region)
//!   must be genuinely free (not used by the kernel image, Limine structures,
//!   or the bitmap itself).

use core::ptr::NonNull;

use super::FrameAllocator;
use crate::mm::addr::{PAGE_SIZE, PhysAddr};
use crate::mm::frame::{OwnedFrame, PhysFrame};

/// A node in the intrusive free list.
///
/// Written into the first 8 bytes of each free frame. The `next` field holds
/// the *physical* address of the next free frame, or `0` for end-of-list.
#[repr(C)]
struct FreeNode {
    next: usize, // PhysAddr of next free frame, or 0
}

/// A physical frame allocator backed by a bitmap and an intrusive free list.
///
/// See the [module-level documentation](self) for the algorithm.
pub struct BitmapAllocator {
    /// One bit per frame: 0 = free, 1 = allocated.
    bitmap: NonNull<[u8]>,

    /// Physical address of the first node in the free list, or 0 if empty.
    free_list_head: usize,

    /// Offset added to a physical address to obtain its virtual address.
    /// Provided by Limine's HHDM response.
    hhdm_offset: usize,

    /// Total number of frames tracked by this allocator.
    total: usize,

    /// Number of currently free frames.
    free: usize,
}

unsafe impl Send for BitmapAllocator {}

impl BitmapAllocator {
    /// Create a new bitmap allocator.
    ///
    /// # Panics
    ///
    /// Panics if `bitmap` is too small to track all frames up to `max_phys_addr`.
    ///
    /// # Safety
    ///
    /// - `bitmap` must be zeroed before this call.
    /// - `bitmap` must have `'static` lifetime. It must remain valid for the
    ///   entire lifetime of the allocator.
    /// - `hhdm_offset` must be the correct HHDM base for the current boot.
    pub unsafe fn new(bitmap: &'static mut [u8], hhdm_offset: usize, max_phys_addr: usize) -> Self {
        let total = (max_phys_addr + PAGE_SIZE - 1) / PAGE_SIZE;

        let required = (total + 7) / 8;
        assert!(
            bitmap.len() >= required,
            "bitmap too small: need {} bytes for {} frames, got {}",
            required,
            total,
            bitmap.len(),
        );

        Self {
            bitmap: NonNull::from(bitmap),
            free_list_head: 0,
            hhdm_offset,
            total,
            free: 0,
        }
    }

    /// Add a usable physical memory region to the allocator.
    ///
    /// Frames in `[base, base + len)` will be marked free and inserted into
    /// the free list. Any sub-page tail is silently ignored.
    ///
    /// # Safety
    ///
    /// - The memory in `[base, base + len)` must be genuinely free — not used
    ///   by the kernel image, Limine data structures, the bitmap, or any other
    ///   allocator.
    /// - The HHDM must map this region so the allocator can write free-list
    ///   pointers into the frames.
    pub unsafe fn add_region(&mut self, base: PhysAddr, len: usize) {
        let start_frame = PhysFrame::containing(
            base.align_up(PAGE_SIZE)
                .expect("region base align_up overflowed"),
        );
        let end_addr = match base.checked_add(len) {
            Some(a) => a,
            None => return, // overflow, skip
        };
        let end_frame = PhysFrame::containing(end_addr);

        let mut frame = start_frame;
        while frame < end_frame {
            // SAFETY: caller guarantees this frame is free and HHDM-mapped.
            unsafe { self.free_frame(frame) };
            frame = match PhysFrame::from_index(frame.index() + 1) {
                Some(f) => f,
                None => break,
            };
        }
    }

    /// Returns the bitmap as a shared byte slice.
    ///
    /// # Safety
    ///
    /// No concurrent mutable access.
    #[inline]
    unsafe fn bitmap(&self) -> &[u8] {
        // SAFETY: caller ensures no aliasing mutable access.
        unsafe { self.bitmap.as_ref() }
    }

    /// Returns the bitmap as a mutable byte slice.
    ///
    /// # Safety
    ///
    /// Exclusive access required.
    #[inline]
    unsafe fn bitmap_mut(&mut self) -> &mut [u8] {
        // SAFETY: we hold &mut self.
        unsafe { self.bitmap.as_mut() }
    }

    /// Returns `true` if `frame` is currently allocated.
    #[inline]
    fn is_allocated(&self, frame: PhysFrame) -> bool {
        let idx = frame.index();
        let byte = idx / 8;
        let bit = idx % 8;
        // SAFETY: bitmap lives for 'static. no concurrent mutation.
        let bitmap = unsafe { self.bitmap() };
        if byte >= bitmap.len() {
            return true;
        }
        bitmap[byte] & (1 << bit) != 0
    }

    /// Mark `frame` as allocated in the bitmap.
    #[inline]
    fn mark_allocated(&mut self, frame: PhysFrame) {
        let idx = frame.index();
        let byte = idx / 8;
        let bit = idx % 8;
        // SAFETY: exclusive access via &mut self.
        let bitmap = unsafe { self.bitmap_mut() };
        bitmap[byte] |= 1 << bit;
    }

    /// Mark `frame` as free in the bitmap.
    #[inline]
    fn mark_free(&mut self, frame: PhysFrame) {
        let idx = frame.index();
        let byte = idx / 8;
        let bit = idx % 8;
        // SAFETY: exclusive access via &mut self.
        let bitmap = unsafe { self.bitmap_mut() };
        bitmap[byte] &= !(1 << bit);
    }

    /// Convert a physical address to a virtual pointer via the HHDM.
    ///
    /// # Safety
    ///
    /// The HHDM must map `phys`.
    #[inline]
    unsafe fn phys_to_virt<T>(&self, phys: usize) -> *mut T {
        (phys + self.hhdm_offset) as *mut T
    }

    /// Push `frame` onto the front of the free list and mark it free.
    ///
    /// # Safety
    ///
    /// - `frame` must be genuinely free (not in use by anyone else).
    /// - The HHDM must map this frame so we can write the list node.
    unsafe fn free_frame(&mut self, frame: PhysFrame) {
        self.mark_free(frame);

        // Write the current head into the frame's first 8 bytes.
        // SAFETY: frame is free and HHDM-mapped; we have exclusive access.
        let node = unsafe {
            self.phys_to_virt::<FreeNode>(frame.base().as_usize())
                .as_mut()
                .expect("frame base is null")
        };
        node.next = self.free_list_head;

        self.free_list_head = frame.base().as_usize();
        self.free += 1;
    }

    /// Pop the first frame off the free list and mark it allocated.
    ///
    /// Returns `None` if the free list is empty.
    fn pop_free(&mut self) -> Option<OwnedFrame> {
        if self.free_list_head == 0 {
            return None;
        }

        let phys = self.free_list_head;

        // Read the next pointer from the frame.
        // SAFETY: free_list_head points to a free, HHDM-mapped frame we own.
        let node = unsafe {
            self.phys_to_virt::<FreeNode>(phys)
                .as_ref()
                .expect("free list node is null")
        };
        self.free_list_head = node.next;

        let frame = PhysFrame::from_index(phys / PAGE_SIZE)
            .expect("free list contained invalid frame address");

        self.mark_allocated(frame);
        self.free -= 1;

        Some(OwnedFrame::new(frame))
    }
}

// SAFETY: The invariants documented on the trait are maintained by the
// bitmap (prevents double-free detection) and the free list (O(1) operations).
impl FrameAllocator for BitmapAllocator {
    #[inline]
    fn allocate(&mut self) -> Option<OwnedFrame> {
        self.pop_free()
    }

    /// # Safety
    ///
    /// `frame` must have been returned by a prior call to [`allocate`](Self::allocate)
    /// on this allocator instance and must not have been deallocated since.
    /// Prefer [`OwnedFrame::free`] over calling this directly.
    #[inline]
    unsafe fn deallocate(&mut self, frame: PhysFrame) {
        assert!(
            self.is_allocated(frame),
            "deallocate called on a frame that is not allocated: {frame:?}",
        );
        // SAFETY: caller guarantees frame was previously allocated and is no
        // longer in use. The HHDM maps all physical frames.
        unsafe { self.free_frame(frame) };
    }

    #[inline]
    fn total_frames(&self) -> usize {
        self.total
    }

    #[inline]
    fn free_frames(&self) -> usize {
        self.free
    }
}

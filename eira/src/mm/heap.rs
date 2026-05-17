//! Kernel heap allocator.
//!
//! A first-fit, coalescing linked-list allocator that implements
//! [`core::alloc::GlobalAlloc`] and serves as the kernel's `#[global_allocator]`.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;

/// Minimum alignment of every allocated block and every `FreeBlock` header.
///
/// 16 bytes satisfies the alignment requirements of all standard Rust types,
/// including SIMD types up to 128-bit and matches with System V ABI requirement
/// for `malloc`.
pub const BLOCK_ALIGN: usize = 16;

/// Minimum size of a free block.
///
/// A free block must be large enough to hold the [`FreeBlock`] header so the
/// list node can be written into the memory it describes. Any allocation
/// smaller than this is rounded up to `MIN_BLOCK_SIZE`.
pub const MIN_BLOCK_SIZE: usize = core::mem::size_of::<FreeBlock>();

#[repr(C)]
pub(crate) struct FreeBlock {
    /// Total size of this free block in bytes, including the header itself.
    ///
    /// Always >= [`MIN_BLOCK_SIZE`] and a multiple of [`BLOCK_ALIGN`].
    pub(crate) size: usize,

    /// The next free block in the sorted free list, or `None` if this is
    /// the last block.
    pub(crate) next: Option<NonNull<FreeBlock>>,
}

// FreeBlock must fit within MIN_BLOCK_SIZE and be aligned to BLOCK_ALIGN.
const _: () = assert!(
    core::mem::size_of::<FreeBlock>() <= MIN_BLOCK_SIZE,
    "FreeBlock header exceeds MIN_BLOCK_SIZE",
);
const _: () = assert!(
    core::mem::align_of::<FreeBlock>() <= BLOCK_ALIGN,
    "FreeBlock alignment exceeds BLOCK_ALIGN",
);

impl FreeBlock {
    /// Returns the address of this block as a `usize`.
    #[inline]
    pub(crate) fn addr(ptr: NonNull<Self>) -> usize {
        ptr.as_ptr() as usize
    }

    /// Returns the exclusive end address of this block (`addr + size`).
    #[inline]
    pub(crate) fn end(ptr: NonNull<Self>, size: usize) -> usize {
        // SAFETY: caller must ensure `ptr` points to a valid FreeBlock with
        // the given `size`.
        Self::addr(ptr) + size
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HeapStats {
    /// Total bytes ever added via [`HeapAllocator::add_memory`].
    pub total_bytes: usize,
    /// Bytes currently available for allocation.
    pub free_bytes: usize,
    /// Number of allocation requests served.
    pub alloc_count: usize,
    /// Number of deallocation requests served.
    pub dealloc_count: usize,
}

impl HeapStats {
    /// Bytes currently in use (allocated but not yet freed).
    #[inline]
    pub fn used_bytes(&self) -> usize {
        self.total_bytes - self.free_bytes
    }

    /// `true` if every allocated byte has been freed.
    #[inline]
    pub fn is_balanced(&self) -> bool {
        self.alloc_count == self.dealloc_count && self.free_bytes == self.total_bytes
    }
}

pub struct HeapAllocator {
    /// Head of the sorted free list, or `None` if the heap is empty.
    head: Option<NonNull<FreeBlock>>,

    /// Diagnostic counters.
    stats: HeapStats,
}

// SAFETY: `HeapAllocator` is only accessed through `&mut self` (enforced by
// the `Mutex` wrapper). Raw pointers inside `FreeBlock` nodes point into the
// heap region, which is valid for the kernel's lifetime.
unsafe impl Send for HeapAllocator {}

impl HeapAllocator {
    /// Construct an empty heap allocator.
    ///
    /// No memory is available until [`add_memory`](Self::add_memory) is called.
    pub const fn new() -> Self {
        Self {
            head: None,
            stats: HeapStats {
                total_bytes: 0,
                free_bytes: 0,
                alloc_count: 0,
                dealloc_count: 0,
            },
        }
    }

    /// Donate a contiguous memory region to the heap.
    ///
    /// The region `[ptr, ptr + size)` is inserted into the free list as one
    /// or more free blocks. If the region is adjacent to an existing free
    /// block, they are merged immediately.
    ///
    /// This method is typically called once during early boot after
    /// `AddressSpace::alloc_and_map` has backed the heap VA range with
    /// physical frames.
    ///
    /// # Panics
    ///
    /// - `ptr` is not aligned to [`BLOCK_ALIGN`].
    /// - `size` is less than [`MIN_BLOCK_SIZE`].
    /// - `ptr` is null.
    ///
    /// # Safety
    ///
    /// - The memory in `[ptr, ptr + size)` must be valid, writable and
    ///   exclusively owned by the caller for the entire lifetime of the
    ///   allocator.
    /// - The region must not overlap with any memory already in the heap or
    ///   with any live allocation.
    /// - The region must remain mapped (backed by physical frames) for the
    ///   entire lifetime of the allocator.
    pub unsafe fn add_memory(&mut self, ptr: *mut u8, size: usize) {
        assert!(!ptr.is_null(), "add_memory: ptr must not be null");
        assert!(
            (ptr as usize) % BLOCK_ALIGN == 0,
            "add_memory: ptr {ptr:p} is not aligned to {BLOCK_ALIGN}",
        );
        assert!(
            size >= MIN_BLOCK_SIZE,
            "add_memory: size {size:#x} is less than MIN_BLOCK_SIZE {MIN_BLOCK_SIZE:#x}",
        );

        let size = size & !(BLOCK_ALIGN - 1);

        let block = unsafe {
            let block_ptr = ptr as *mut FreeBlock;
            block_ptr.write(FreeBlock { size, next: None });
            NonNull::new_unchecked(block_ptr)
        };

        self.stats.total_bytes += size;
        self.stats.free_bytes += size;

        unsafe { self.insert_and_coalesce(block, size) };
    }

    /// Returns a snapshot of the current heap statistics.
    #[inline]
    pub fn stats(&self) -> HeapStats {
        self.stats
    }

    /// Returns the number of free bytes currently available.
    #[inline]
    pub fn free_bytes(&self) -> usize {
        self.stats.free_bytes
    }

    /// Returns the total bytes ever donated via [`add_memory`](Self::add_memory).
    #[inline]
    pub fn total_bytes(&self) -> usize {
        self.stats.total_bytes
    }

    /// Returns `true` if the free list is empty (heap fully allocated or
    /// never initialised).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> Option<NonNull<u8>> {
        debug_assert!(size > 0, "alloc: size must be > 0");
        debug_assert!(
            align.is_power_of_two(),
            "alloc: align must be a power of two"
        );

        let align = align.max(BLOCK_ALIGN);
        let size = size.max(MIN_BLOCK_SIZE).next_multiple_of(BLOCK_ALIGN);

        let mut prev_next: *mut Option<NonNull<FreeBlock>> = &mut self.head;

        loop {
            let current = unsafe { *prev_next };

            let block = match current {
                Some(b) => b,
                None => return None,
            };

            let block_addr = FreeBlock::addr(block);
            let block_size = unsafe { (*block.as_ptr()).size };

            let alloc_start = align_up(block_addr, align);
            let padding = alloc_start - block_addr;

            if padding > 0 && padding < MIN_BLOCK_SIZE {
                prev_next = unsafe { &mut (*block.as_ptr()).next };
                continue;
            }

            let total_needed = match padding.checked_add(size) {
                Some(t) => t,
                None => {
                    prev_next = unsafe { &mut (*block.as_ptr()).next };
                    continue;
                }
            };

            if block_size < total_needed {
                prev_next = unsafe { &mut (*block.as_ptr()).next };
                continue;
            }

            let block_next = unsafe { (*block.as_ptr()).next };

            if padding > 0 {
                unsafe {
                    (*block.as_ptr()).size = padding;
                    (*block.as_ptr()).next = block_next;
                }
                prev_next = unsafe { &mut (*block.as_ptr()).next };
            } else {
                unsafe { *prev_next = block_next };
            }

            let remainder = block_size - padding - size;
            if remainder >= MIN_BLOCK_SIZE {
                let remainder_addr = alloc_start + size;
                let remainder_ptr = unsafe {
                    let p = remainder_addr as *mut FreeBlock;
                    p.write(FreeBlock {
                        size: remainder,
                        next: block_next,
                    });
                    NonNull::new_unchecked(p)
                };
                unsafe { *prev_next = Some(remainder_ptr) };
            }

            let freed_to_remainder = if remainder >= MIN_BLOCK_SIZE {
                remainder
            } else {
                0
            };
            let consumed = block_size - padding - freed_to_remainder;
            self.stats.free_bytes -= consumed;
            self.stats.alloc_count += 1;

            return Some(unsafe { NonNull::new_unchecked(alloc_start as *mut u8) });
        }
    }

    /// Return a previously allocated region to the heap.
    ///
    /// The freed region is inserted back into the sorted free list and merged
    /// with any adjacent free blocks to prevent fragmentation.
    ///
    /// # Safety
    ///
    /// - `ptr` must have been returned by a prior call to [`alloc`](Self::alloc)
    ///   on **this** allocator instance.
    /// - `size` and `align` must exactly match the values passed to that call.
    /// - `ptr` must not have been deallocated since.
    /// - No live references into `[ptr, ptr + size)` may exist after this call.
    pub unsafe fn dealloc(&mut self, ptr: NonNull<u8>, size: usize, _align: usize) {
        let block_size = size.max(MIN_BLOCK_SIZE).next_multiple_of(BLOCK_ALIGN);

        let block = unsafe { NonNull::new_unchecked(ptr.as_ptr() as *mut FreeBlock) };

        self.stats.free_bytes += block_size;
        self.stats.dealloc_count += 1;

        unsafe { self.insert_and_coalesce(block, block_size) };
    }

    pub(crate) unsafe fn insert_and_coalesce(&mut self, block: NonNull<FreeBlock>, size: usize) {
        let block_addr = FreeBlock::addr(block);

        let mut prev_next: *mut Option<NonNull<FreeBlock>> = &mut self.head;
        let mut prev_ptr: Option<NonNull<FreeBlock>> = None;

        loop {
            let next = unsafe { *prev_next };

            match next {
                Some(next_block) if FreeBlock::addr(next_block) < block_addr => {
                    prev_ptr = Some(next_block);
                    prev_next = unsafe { &mut (*next_block.as_ptr()).next };
                }
                _ => break,
            }
        }

        let next_block: Option<NonNull<FreeBlock>> = unsafe { *prev_next };

        unsafe {
            block.as_ptr().write(FreeBlock {
                size,
                next: next_block,
            });
        }

        unsafe { *prev_next = Some(block) };

        if let Some(right) = next_block {
            let block_end = FreeBlock::end(block, size);
            let right_addr = FreeBlock::addr(right);

            if block_end == right_addr {
                let right_size = unsafe { (*right.as_ptr()).size };
                let right_next = unsafe { (*right.as_ptr()).next };

                unsafe {
                    (*block.as_ptr()).size = size + right_size;
                    (*block.as_ptr()).next = right_next;
                }
            }
        }

        if let Some(left) = prev_ptr {
            let left_size = unsafe { (*left.as_ptr()).size };
            let left_end = FreeBlock::end(left, left_size);

            if left_end == block_addr {
                let merged_next = unsafe { (*block.as_ptr()).next };
                let merged_size = unsafe { (*block.as_ptr()).size };

                unsafe {
                    (*left.as_ptr()).size = left_size + merged_size;
                    (*left.as_ptr()).next = merged_next;
                }
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn free_list_len(&self) -> usize {
        let mut count = 0usize;
        let mut current = self.head;
        while let Some(block) = current {
            count += 1;
            current = unsafe { (*block.as_ptr()).next };
        }
        count
    }
}

impl Default for HeapAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for HeapAllocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HeapAllocator")
            .field("free_bytes", &self.stats.free_bytes)
            .field("total_bytes", &self.stats.total_bytes)
            .field("free_blocks", &self.free_list_len())
            .field("alloc_count", &self.stats.alloc_count)
            .field("dealloc_count", &self.stats.dealloc_count)
            .finish()
    }
}

#[inline]
pub(crate) const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

pub struct LockedHeap(spin::Mutex<HeapAllocator>);

impl LockedHeap {
    pub const fn new() -> Self {
        Self(spin::Mutex::new(HeapAllocator::new()))
    }

    pub fn lock(&self) -> spin::MutexGuard<'_, HeapAllocator> {
        self.0.lock()
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut heap = self.lock();
        match heap.alloc(layout.size(), layout.align()) {
            Some(ptr) => ptr.as_ptr(),
            None => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if let Some(p) = NonNull::new(ptr) {
            let mut heap = self.lock();
            unsafe { heap.dealloc(p, layout.size(), layout.align()) }
        }
    }
}

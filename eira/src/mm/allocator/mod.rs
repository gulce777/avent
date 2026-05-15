//! Physical frame allocator interface and implementations.
//!
//! Code that needs to allocate frames depends only on the [`FrameAllocator`]
//! trait. Not on any concrete implementation!

pub mod bitmap;

use super::frame::PhysFrame;
use crate::mm::addr::PAGE_SIZE;

/// A physical memory frame allocator.
///
/// Implementors hand out and reclaim [`PhysFrame`]s at page granularity.
/// The trait is intentionally minimal, higher-level alloctors (slab, buddy, etc.)
/// are built on TOP of this, not INSIDE it.
///
/// # Safety
///
/// Implementations must uphold the following invariants:
///
/// - [`allocate`](FrameAllocator::allocate) must return a frame that is not currently
///   allocated by any other call.
/// - [`deallocate`](FrameAllocator::deallocate) must only be called with a
///   frame that previously returned by [`allocate`](FrameAllocator::allocate)
///   and has not yet ben deallocated.
/// - Violating either invariant is **undefined behaviour** (physical aliasing).
pub unsafe trait FrameAllocator {
    /// Allocate a single 4 KiB physical frame.
    ///
    /// Returns `None` if physical memory is exhausted.
    fn allocate(&mut self) -> Option<PhysFrame<{ PAGE_SIZE }>>;

    /// Return a previously allocated frame to the allocator.
    ///
    /// # Safety
    ///
    /// `frame` must have been returned by a prior call to
    /// [`allocate`](Self::allocate) on this allocator instance, and must not
    /// have been deallocated since.
    unsafe fn deallocate(&mut self, frame: PhysFrame<{ PAGE_SIZE }>);

    /// Allocate `count` contiguous frames.
    ///
    /// The default implementation falls back to `count` individual calls and
    /// is therefore **not** guaranteed to return contiguous frames. Allocators
    /// that can provide contiguous allocations should override this method.
    ///
    /// Returns `None` if the request cannot be satisfied.
    fn allocate_contiguous(&mut self, count: usize) -> Option<PhysFrame<{ PAGE_SIZE }>> {
        if count == 1 { self.allocate() } else { None }
    }

    /// Returns the total number of frames managed by this allocator.
    fn total_frames(&self) -> usize;

    /// Returns the number of currently free frames.
    fn free_frames(&self) -> usize;

    /// Returns the number of currently allocated frames.
    #[inline]
    fn used_frames(&self) -> usize {
        self.total_frames() - self.free_frames()
    }
}

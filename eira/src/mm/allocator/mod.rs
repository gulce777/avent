//! Physical frame allocator interface and implementations.
//!
//! Code that needs to allocate frames depends only on the [`FrameAllocator`]
//! trait. Not on any concrete implementation.

pub mod bitmap;

use super::frame::{OwnedFrame, PhysFrame};
use crate::mm::addr::PAGE_SIZE;

/// A physical memory frame allocator.
///
/// Implementors hand out and reclaim [`PhysFrame`]s at page granularity.
/// The trait is intentionally minimal, higher-level allocators (slab, buddy, etc.)
/// are built on TOP of this, not INSIDE it.
pub trait FrameAllocator {
    /// Allocate a single 4 KiB physical frame.
    ///
    /// Returns `None` if physical memory is exhausted.
    fn allocate(&mut self) -> Option<OwnedFrame>;

    /// Return a previously allocated frame to the allocator.
    ///
    /// # Safety
    ///
    /// `frame` must have been returned by a prior call to
    /// [`allocate`](Self::allocate) on this allocator instance, and must not
    /// have been deallocated since. Prefer [`OwnedFrame::free`] over calling this
    /// directly.
    #[allow(dead_code)]
    unsafe fn deallocate(&mut self, frame: PhysFrame<{ PAGE_SIZE }>);

    /// Allocate `count` contiguous frames.
    ///
    /// The default implementation only satisfies `count == 1` and returns
    /// `None` for anything larger. Allocators that can provide genuinely
    /// contiguous frames should override this method.
    #[allow(dead_code)]
    fn allocate_contiguous(&mut self, count: usize) -> Option<OwnedFrame> {
        if count == 1 { self.allocate() } else { None }
    }

    /// Returns the total number of frames managed by this allocator.
    fn total_frames(&self) -> usize;

    /// Returns the number of currently free frames.
    fn free_frames(&self) -> usize;

    /// Returns the number of currently allocated frames.
    #[inline]
    #[allow(dead_code)]
    fn used_frames(&self) -> usize {
        self.total_frames() - self.free_frames()
    }
}

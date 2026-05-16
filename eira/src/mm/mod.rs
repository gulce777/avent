//! Physical memory management.
//!
//! Provides the building blocks for all memory operations in the kernel.

pub mod addr;
pub mod allocator;
pub mod frame;
pub mod init;

#[cfg(feature = "kernel-tests")]
mod tests;

pub use addr::{HUGE_PAGE_SIZE, LARGE_PAGE_SIZE, PAGE_SIZE, PhysAddr, VirtAddr};
pub use allocator::FrameAllocator;
pub use frame::{Frame1G, Frame2M, Frame4K, FrameRange, OwnedFrame, PhysFrame};
pub use init::{allocate, deallocate, free_frames, total_frames};

//! Physical memory management.
//!
//! Provides the building blocks for all memory operations in the kernel.

pub mod addr;
pub mod address_space;
pub mod allocator;
pub mod frame;
pub mod heap;
pub mod init;
pub mod paging;
pub mod vm;

#[cfg(feature = "kernel-tests")]
mod heap_tests;
#[cfg(feature = "kernel-tests")]
mod tests;

pub use addr::{PAGE_SIZE, PhysAddr, VirtAddr};
pub use allocator::FrameAllocator;
#[allow(unused_imports)]
pub use frame::{Frame4K, FrameRange, OwnedFrame, PhysFrame};
#[allow(unused_imports)]
pub use init::{allocate, deallocate, free_frames, total_frames};

//! Physical memory management.

pub mod addr;
pub mod allocator;
pub mod frame;
pub mod init;

pub use addr::{HUGE_PAGE_SIZE, LARGE_PAGE_SIZE, PAGE_SIZE, PhysAddr, VirtAddr};
pub use allocator::FrameAllocator;
pub use frame::{Frame1G, Frame2M, Frame4K, FrameRange, PhysFrame};
pub use init::{allocate, deallocate, free_frames, total_frames};

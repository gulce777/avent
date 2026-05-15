//! Early boot physical memory initialization.
//!
//! # Bootstrap problem
//!
//! The bitmap allocator needs memory to store its bitmap, but we have no
//! allocator yet to give it that memory. We solve this by carving the bitmap
//! directly out of the first usable physical memory region, *before* handing
//! any memory to the allocator.

use limine::memmap::MEMMAP_USABLE;
use limine::request::{HhdmRespData, MemmapRespData, Response};
use log;
use spin::Mutex;

use super::addr::{PAGE_SIZE, PhysAddr};
use super::allocator::FrameAllocator;
use super::allocator::bitmap::BitmapAllocator;
use super::frame::PhysFrame;

/// The kernel-global physical frame allocator.
pub static FRAME_ALLOCATOR: Mutex<Option<BitmapAllocator>> = Mutex::new(None);

/// Initialize the physical memory allocator from the Limine memory map.
///
/// Must be called exactly once, early in [`kernel_main`](crate::kernel_main),
/// before any call to [`allocate`].
///
/// # Panics
///
/// Panics if:
/// - No usable memory region is large enough to hold the bitmap.
/// - The Limine memory map contains no usable entries.
///
/// # Safety
///
/// - `memmap` and `hhdm` must be valid Limine responses for the current boot.
/// - No other code may access physical memory outside the kernel image until
///   this function returns.
pub unsafe fn init(memmap: &Response<MemmapRespData>, hhdm: &Response<HhdmRespData>) {
    let hhdm_offset = hhdm.offset as usize;

    let max_phys = memmap
        .entries()
        .iter()
        .filter(|e| e.type_ == MEMMAP_USABLE)
        .map(|e| (e.base + e.length) as usize)
        .max()
        .expect("no usable memory regions");

    let total_frames = (max_phys + PAGE_SIZE - 1) / PAGE_SIZE;
    let bitmap_bytes = (total_frames + 7) / 8;
    let bitmap_pages = (bitmap_bytes + PAGE_SIZE - 1) / PAGE_SIZE;
    let bitmap_size = bitmap_pages * PAGE_SIZE;

    log::debug!(
        "physical memory: max={:#x}  frames={}  bitmap={} bytes ({} pages)",
        max_phys,
        total_frames,
        bitmap_bytes,
        bitmap_pages,
    );

    // ── Step 2: carve the bitmap out of the first suitable usable region ───────
    let bitmap_phys = memmap
        .entries()
        .iter()
        .find(|e| e.type_ == MEMMAP_USABLE && e.length as usize >= bitmap_size)
        .map(|e| e.base as usize)
        .expect("no usable region large enough for the physical memory bitmap");

    log::debug!(
        "bitmap carved at {:#x} ({} bytes)",
        bitmap_phys,
        bitmap_size
    );

    // SAFETY:
    // - The HHDM maps all physical memory, so `bitmap_phys + hhdm_offset` is valid.
    // - The region is usable (not used by the kernel or Limine structures).
    // - We will not add this region to the allocator until after construction,
    //   preventing aliasing between the bitmap and allocatable frames.
    // - We cast to 'static because the allocator must own the bitmap for its
    //   entire lifetime; the kernel never exits, so this is sound.
    let bitmap: &'static mut [u8] = unsafe {
        core::slice::from_raw_parts_mut((bitmap_phys + hhdm_offset) as *mut u8, bitmap_size)
    };

    bitmap.fill(0x00);

    // SAFETY: bitmap is zeroed, correctly sized, and lives for 'static
    let mut allocator = unsafe { BitmapAllocator::new(bitmap, hhdm_offset, max_phys) };

    let bitmap_end = bitmap_phys + bitmap_size;

    for entry in memmap.entries().iter().filter(|e| e.type_ == MEMMAP_USABLE) {
        let region_base = entry.base as usize;
        let region_end = region_base + entry.length as usize;

        if region_base < bitmap_phys {
            let len = bitmap_phys.min(region_end) - region_base;
            if len > 0 {
                if let Some(base) = PhysAddr::new(region_base) {
                    // SAFETY: region is usable and not the bitmap.
                    unsafe { allocator.add_region(base, len) };
                }
            }
        }

        if region_end > bitmap_end {
            let start = bitmap_end.max(region_base);
            let len = region_end - start;
            if len > 0 {
                if let Some(base) = PhysAddr::new(start) {
                    // SAFETY: region is usable and not the bitmap.
                    unsafe { allocator.add_region(base, len) };
                }
            }
        }
    }

    log::info!(
        "frame allocator ready: {}/{} frames free ({} MiB / {} MiB)",
        allocator.free_frames(),
        allocator.total_frames(),
        allocator.free_frames() * PAGE_SIZE / (1024 * 1024),
        allocator.total_frames() * PAGE_SIZE / (1024 * 1024),
    );

    *FRAME_ALLOCATOR.lock() = Some(allocator);
}

pub fn allocate() -> PhysFrame {
    FRAME_ALLOCATOR
        .lock()
        .as_mut()
        .expect("frame allocator not initialised")
        .allocate()
        .expect("out of physical memory")
}

pub unsafe fn deallocate(frame: PhysFrame) {
    // SAFETY: caller upholds the frame allocator invariants.
    unsafe {
        FRAME_ALLOCATOR
            .lock()
            .as_mut()
            .expect("frame allocator not initialised")
            .deallocate(frame);
    }
}

pub fn free_frames() -> usize {
    FRAME_ALLOCATOR
        .lock()
        .as_ref()
        .expect("frame allocator not initialised")
        .free_frames()
}

pub fn total_frames() -> usize {
    FRAME_ALLOCATOR
        .lock()
        .as_ref()
        .expect("frame allocator not initialised")
        .total_frames()
}

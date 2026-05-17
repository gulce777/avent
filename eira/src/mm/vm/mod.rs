pub mod region;
#[cfg(feature = "kernel-tests")]
pub mod tests;

use crate::mm::addr::VirtAddr;
pub use region::{Region, RegionAllocator};

/// Start of the kernel heap virtual region.
///
/// The heap is initially empty; the heap allocator maps physical frames into
/// this range on demand.
pub const HEAP_START: usize = 0xffff_c000_0000_0000;

/// Exclusive end of the kernel heap region (16 TiB).
pub const HEAP_END: usize = 0xffff_d000_0000_0000;

/// Start of the general kernel virtual memory region.
///
/// Used for vmalloc-style mappings: framebuffers, MMIO, per-CPU stacks, and
/// other kernel objects that need virtually-contiguous memory but may be
/// physically discontiguous.
pub const KERNEL_VM_START: usize = 0xffff_d000_0000_0000;

/// Exclusive end of the general kernel VM region (16 TiB).
pub const KERNEL_VM_END: usize = 0xffff_e000_0000_0000;

/// Returns `true` if `addr` falls within the kernel heap VA region.
#[inline]
#[allow(dead_code)]
pub fn is_heap_addr(addr: VirtAddr) -> bool {
    let raw = addr.as_usize();
    raw >= HEAP_START && raw < HEAP_END
}

/// Returns `true` if `addr` falls within the general kernel VM region.
#[inline]
#[allow(dead_code)]
pub fn is_kernel_vm_addr(addr: VirtAddr) -> bool {
    let raw = addr.as_usize();
    raw >= KERNEL_VM_START && raw < KERNEL_VM_END
}

/// Build a [`RegionAllocator`] pre-loaded with the kernel heap VA range.
///
/// Called once by [`AddressSpace::new_kernel`] during early boot.
pub(super) fn heap_region_allocator() -> RegionAllocator {
    let mut ra = RegionAllocator::new();
    let base = unsafe { VirtAddr::new_unchecked(HEAP_START) };
    let size = HEAP_END - HEAP_START;
    ra.add_region(Region::new(base, size))
        .expect("initial heap region registration failed");
    ra
}

/// Build a [`RegionAllocator`] pre-loaded with the general kernel VM range.
///
/// Called once by [`AddressSpace::new_kernel`] during early boot.
pub(super) fn kernel_vm_region_allocator() -> RegionAllocator {
    let mut ra = RegionAllocator::new();
    let base = unsafe { VirtAddr::new_unchecked(KERNEL_VM_START) };
    let size = KERNEL_VM_END - KERNEL_VM_START;
    ra.add_region(Region::new(base, size))
        .expect("initial kernel VM region registration failed");
    ra
}

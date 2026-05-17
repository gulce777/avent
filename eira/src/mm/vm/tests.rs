//! Integration tests for [`AddressSpace`] and [`RegionAllocator`].

use crate::mm::address_space::{AllocKind, KernelAddressSpace};
use crate::mm::paging::PageFlags;
use crate::mm::{self, PAGE_SIZE, VirtAddr};
use crate::test::{TestKind, TestResult};
use crate::{kassert, kassert_eq, kernel_test};

kernel_test!(region_alloc_basic, TestKind::Unit, {
    use crate::mm::vm::region::{Region, RegionAllocator};

    let base = unsafe { VirtAddr::new_unchecked(0xffff_d000_0000_0000) };
    let mut ra = RegionAllocator::new();
    ra.add_region(Region::new(base, 16 * PAGE_SIZE))
        .expect("add_region failed");

    let r = ra.alloc(4 * PAGE_SIZE, PAGE_SIZE).expect("alloc failed");
    kassert_eq!(r.size, 4 * PAGE_SIZE);
    kassert!(r.base.is_aligned(PAGE_SIZE));

    kassert_eq!(ra.free_bytes(), 12 * PAGE_SIZE);
});

kernel_test!(region_alloc_free_merge, TestKind::Unit, {
    use crate::mm::vm::region::{Region, RegionAllocator};

    let base = unsafe { VirtAddr::new_unchecked(0xffff_d000_0000_0000) };
    let mut ra = RegionAllocator::new();
    ra.add_region(Region::new(base, 8 * PAGE_SIZE))
        .expect("add_region failed");

    let r = ra.alloc(8 * PAGE_SIZE, PAGE_SIZE).expect("alloc failed");
    kassert_eq!(ra.free_bytes(), 0);

    ra.free(r).expect("free failed");
    kassert_eq!(ra.free_bytes(), 8 * PAGE_SIZE);
    kassert_eq!(ra.free_region_count(), 1);
});

kernel_test!(region_alloc_alignment, TestKind::Unit, {
    use crate::mm::vm::region::{Region, RegionAllocator};

    let align_2m = 2 * 1024 * 1024usize;
    let base_raw = 0xffff_d000_0000_0000usize + PAGE_SIZE;
    let base = unsafe { VirtAddr::new_unchecked(base_raw) };
    let padding = align_2m - PAGE_SIZE;
    let region_size = padding + align_2m;

    let mut ra = RegionAllocator::new();
    ra.add_region(Region::new(base, region_size))
        .expect("add_region failed");

    let r = ra.alloc(align_2m, align_2m).expect("alloc failed");
    kassert!(r.base.is_aligned(align_2m));
    kassert_eq!(r.size, align_2m);
});

kernel_test!(region_out_of_vm, TestKind::Unit, {
    use crate::mm::vm::region::{Region, RegionAllocator, RegionError};

    let base = unsafe { VirtAddr::new_unchecked(0xffff_d000_0000_0000) };
    let mut ra = RegionAllocator::new();
    ra.add_region(Region::new(base, PAGE_SIZE))
        .expect("add_region failed");

    let _ = ra.alloc(PAGE_SIZE, PAGE_SIZE).expect("first alloc failed");

    let result = ra.alloc(PAGE_SIZE, PAGE_SIZE);
    kassert!(result == Err(RegionError::OutOfVirtualMemory));
});

kernel_test!(address_space_alloc_and_map_4_pages, TestKind::Physical, {
    let mut kspace = KernelAddressSpace::new_kernel();

    let warmup = kspace
        .alloc_and_map(
            PAGE_SIZE,
            PAGE_SIZE,
            PageFlags::kernel_data(),
            AllocKind::KernelVm,
        )
        .expect("warmup alloc_and_map failed");

    kspace
        .unmap_and_dealloc(warmup, AllocKind::KernelVm)
        .expect("warmup unmap failed");

    let free_before = mm::free_frames();

    let region = kspace
        .alloc_and_map(
            4 * PAGE_SIZE,
            PAGE_SIZE,
            PageFlags::kernel_data(),
            AllocKind::KernelVm,
        )
        .expect("alloc_and_map failed");

    kassert_eq!(region.size, 4 * PAGE_SIZE);
    kassert_eq!(mm::free_frames(), free_before - 4);

    let base = region.base.as_usize();
    for i in 0..4usize {
        let virt = unsafe { VirtAddr::new_unchecked(base + i * PAGE_SIZE) };
        kassert!(kspace.translate(virt).is_some());
    }

    let phys: [usize; 4] = core::array::from_fn(|i| {
        let virt = unsafe { VirtAddr::new_unchecked(base + i * PAGE_SIZE) };
        kspace.translate(virt).unwrap().0.as_usize()
    });
    for i in 0..4 {
        for j in (i + 1)..4 {
            kassert!(phys[i] != phys[j]);
        }
    }

    kspace
        .unmap_and_dealloc(region, AllocKind::KernelVm)
        .expect("unmap_and_dealloc failed");

    kassert_eq!(mm::free_frames(), free_before);
});

kernel_test!(address_space_flags_roundtrip, TestKind::Physical, {
    use crate::mm::paging::PageFlags;

    let mut kspace = KernelAddressSpace::new_kernel();

    let region = kspace
        .alloc_and_map(
            PAGE_SIZE,
            PAGE_SIZE,
            PageFlags::kernel_data(),
            AllocKind::KernelVm,
        )
        .expect("alloc_and_map failed");

    let virt = region.base;
    let (_, flags) = kspace.translate(virt).expect("translation failed");

    kassert!(flags.contains(PageFlags::READ));
    kassert!(flags.contains(PageFlags::WRITE));
    kassert!(!flags.contains(PageFlags::EXECUTE));

    kspace
        .unmap_and_dealloc(region, AllocKind::KernelVm)
        .expect("unmap failed");
});

kernel_test!(address_space_remap_flags, TestKind::Physical, {
    use crate::mm::paging::PageFlags;

    let mut kspace = KernelAddressSpace::new_kernel();

    let region = kspace
        .alloc_and_map(
            PAGE_SIZE,
            PAGE_SIZE,
            PageFlags::kernel_data(),
            AllocKind::KernelVm,
        )
        .expect("alloc_and_map failed");

    let virt = region.base;

    kspace
        .remap(virt, PageFlags::kernel_code())
        .expect("remap failed")
        .flush();

    let (_, flags) = kspace
        .translate(virt)
        .expect("translation after remap failed");
    kassert!(flags.contains(PageFlags::READ));
    kassert!(flags.contains(PageFlags::EXECUTE));
    kassert!(!flags.contains(PageFlags::WRITE));

    kspace
        .unmap_and_dealloc(region, AllocKind::KernelVm)
        .expect("unmap failed");
});

kernel_test!(address_space_translate_unmapped, TestKind::Physical, {
    let kspace = KernelAddressSpace::new_kernel();

    let unmapped = unsafe { VirtAddr::new_unchecked(0xffff_d000_dead_0000) };
    kassert!(kspace.translate(unmapped).is_none());
});

kernel_test!(address_space_virt_pool_accounting, TestKind::Physical, {
    let mut kspace = KernelAddressSpace::new_kernel();

    let before = kspace.kernel_vm_free_bytes();
    kassert!(before > 0);

    let region = kspace
        .alloc_virt(8 * PAGE_SIZE, PAGE_SIZE, AllocKind::KernelVm)
        .expect("alloc_virt failed");

    kassert_eq!(kspace.kernel_vm_free_bytes(), before - 8 * PAGE_SIZE);

    kspace
        .free_virt(region, AllocKind::KernelVm)
        .expect("free_virt failed");

    kassert_eq!(kspace.kernel_vm_free_bytes(), before);
});

use crate::arch::{Arch, Platform};
use crate::mm::{
    VirtAddr,
    paging::{MapError, Mapper, PageFlags},
};
use crate::{kassert, kassert_eq, kernel_test, test::TestKind};

// Helper
fn virt(addr: usize) -> VirtAddr {
    unsafe { VirtAddr::new_unchecked(addr) }
}

fn do_map(
    mapper: &mut impl crate::mm::paging::Mapper,
    v: VirtAddr,
    frame: crate::mm::OwnedFrame,
    flags: PageFlags,
) -> crate::mm::paging::TlbFlush {
    mapper
        .map(v, frame, flags)
        .map_err(|(_, e)| e)
        .expect("map should succeed")
}

kernel_test!(map_then_translate, TestKind::Physical, {
    let v = virt(0x0000_1000_0000_0000);
    let frame = crate::mm::allocate();
    let phys = frame.base();
    let mut mapper = Platform::new_test_mapper();

    let flush = mapper
        .map(v, frame, PageFlags::kernel_data())
        .map_err(|(_, e)| e)
        .expect("map failed");
    flush.flush();

    let (got_phys, flags) = mapper.translate(v).expect("translate returned None");
    kassert_eq!(got_phys.as_usize(), phys.as_usize());
    kassert!(flags.contains(PageFlags::WRITE));
    kassert!(!flags.contains(PageFlags::EXECUTE));
});

kernel_test!(translate_unmapped_returns_none, TestKind::Physical, {
    let v = virt(0x0000_1001_0000_0000);
    let mapper = Platform::new_test_mapper();
    kassert!(mapper.translate(v).is_none());
});

kernel_test!(double_map_returns_already_mapped, TestKind::Physical, {
    let v = virt(0x0000_1002_0000_0000);
    let frame_a = crate::mm::allocate();
    let frame_b = crate::mm::allocate();
    let mut mapper = Platform::new_test_mapper();

    mapper
        .map(v, frame_a, PageFlags::kernel_data())
        .expect("first map failed")
        .flush();

    let result = mapper.map(v, frame_b, PageFlags::kernel_data());
    kassert!(matches!(result, Err((_, MapError::AlreadyMapped))));

    if let Err((returned_frame, _)) = result {
        crate::mm::deallocate(returned_frame);
    }
});

kernel_test!(remap_after_unmap, TestKind::Physical, {
    let v = virt(0x0000_1003_0000_0000);
    let frame_a = crate::mm::allocate();
    let frame_b = crate::mm::allocate();
    let phys_b = frame_b.base();
    let mut mapper = Platform::new_test_mapper();

    mapper
        .map(v, frame_a, PageFlags::kernel_data())
        .expect("first map failed")
        .flush();
    let (returned, flush) = mapper.unmap(v).expect("unmap failed");
    flush.flush();
    crate::mm::deallocate(returned);

    mapper
        .map(v, frame_b, PageFlags::kernel_code())
        .expect("second map failed")
        .flush();

    let (got_phys, flags) = mapper.translate(v).expect("translate after remap failed");
    kassert_eq!(got_phys.as_usize(), phys_b.as_usize());
    kassert!(flags.contains(PageFlags::EXECUTE));
    kassert!(!flags.contains(PageFlags::WRITE));
});

kernel_test!(translate_preserves_page_offset, TestKind::Physical, {
    let base = virt(0x0000_1004_0000_0000);
    let frame = crate::mm::allocate();
    let phys_base = frame.base();
    let mut mapper = Platform::new_test_mapper();

    mapper
        .map(base, frame, PageFlags::kernel_data())
        .expect("map failed")
        .flush();

    let v_offset = virt(0x0000_1004_0000_0550);
    let (got_phys, _) = mapper
        .translate(v_offset)
        .expect("translate with offset failed");
    kassert_eq!(got_phys.as_usize(), phys_base.as_usize() + 0x550);
});

kernel_test!(user_data_flags_roundtrip, TestKind::Physical, {
    let v = virt(0x0000_2000_0000_0000);
    let frame = crate::mm::allocate();
    let mut mapper = Platform::new_test_mapper();

    do_map(&mut mapper, v, frame, PageFlags::user_data()).flush();

    let (_, flags) = mapper.translate(v).expect("translate failed");
    kassert!(flags.contains(PageFlags::READ));
    kassert!(flags.contains(PageFlags::WRITE));
    kassert!(flags.contains(PageFlags::USER));
    kassert!(!flags.contains(PageFlags::EXECUTE));
    kassert!(!flags.contains(PageFlags::GLOBAL));
});

kernel_test!(user_code_flags_roundtrip, TestKind::Physical, {
    let v = virt(0x0000_2001_0000_0000);
    let frame = crate::mm::allocate();
    let mut mapper = Platform::new_test_mapper();

    do_map(&mut mapper, v, frame, PageFlags::user_code()).flush();

    let (_, flags) = mapper.translate(v).expect("translate failed");
    kassert!(flags.contains(PageFlags::READ));
    kassert!(flags.contains(PageFlags::EXECUTE));
    kassert!(flags.contains(PageFlags::USER));
    kassert!(!flags.contains(PageFlags::WRITE));
    kassert!(!flags.contains(PageFlags::GLOBAL));
});

kernel_test!(uncached_flag_roundtrip, TestKind::Physical, {
    let v = virt(0x0000_2002_0000_0000);
    let frame = crate::mm::allocate();
    let mut mapper = Platform::new_test_mapper();

    do_map(
        &mut mapper,
        v,
        frame,
        PageFlags::kernel_data().or(PageFlags::UNCACHED),
    )
    .flush();

    let (_, flags) = mapper.translate(v).expect("translate failed");
    kassert!(flags.contains(PageFlags::UNCACHED));
});

kernel_test!(map_non_canonical_returns_error, TestKind::Physical, {
    use crate::mm::paging::MapError;

    let v = unsafe { VirtAddr::new_unchecked(0x0000_8000_0000_0000) };
    let frame = crate::mm::allocate();
    let mut mapper = Platform::new_test_mapper();

    let result = mapper.map(v, frame, PageFlags::kernel_data());
    kassert!(matches!(
        result,
        Err((_, MapError::UnalignedAddress)) | Err((_, MapError::FrameAllocationFailed))
    ));

    if let Err((returned, _)) = result {
        crate::mm::deallocate(returned);
    }
});

kernel_test!(kernel_data_is_global, TestKind::Physical, {
    let v = virt(0x0000_3000_0000_0000);
    let frame = crate::mm::allocate();
    let mut mapper = Platform::new_test_mapper();

    do_map(&mut mapper, v, frame, PageFlags::kernel_data()).flush();

    let (_, flags) = mapper.translate(v).expect("translate failed");
    kassert!(flags.contains(PageFlags::GLOBAL));
    kassert!(!flags.contains(PageFlags::USER));
});

kernel_test!(kernel_code_is_global, TestKind::Physical, {
    let v = virt(0x0000_3001_0000_0000);
    let frame = crate::mm::allocate();
    let mut mapper = Platform::new_test_mapper();

    do_map(&mut mapper, v, frame, PageFlags::kernel_code()).flush();

    let (_, flags) = mapper.translate(v).expect("translate failed");
    kassert!(flags.contains(PageFlags::GLOBAL));
    kassert!(flags.contains(PageFlags::EXECUTE));
    kassert!(!flags.contains(PageFlags::WRITE));
    kassert!(!flags.contains(PageFlags::USER));
});

kernel_test!(unmap_then_remap_different_flags, TestKind::Physical, {
    let v = virt(0x0000_4000_0000_0000);
    let frame_a = crate::mm::allocate();
    let frame_b = crate::mm::allocate();
    let phys_b = frame_b.base();
    let mut mapper = Platform::new_test_mapper();

    do_map(&mut mapper, v, frame_a, PageFlags::kernel_data()).flush();

    let (_, flags_before) = mapper.translate(v).expect("translate failed");
    kassert!(flags_before.contains(PageFlags::WRITE));
    kassert!(!flags_before.contains(PageFlags::EXECUTE));

    let (returned, flush) = mapper.unmap(v).expect("unmap failed");
    flush.flush();
    crate::mm::deallocate(returned);

    do_map(&mut mapper, v, frame_b, PageFlags::kernel_code()).flush();

    let (got_phys, flags_after) = mapper.translate(v).expect("translate after remap failed");
    kassert_eq!(got_phys.as_usize(), phys_b.as_usize());
    kassert!(!flags_after.contains(PageFlags::WRITE));
    kassert!(flags_after.contains(PageFlags::EXECUTE));
});

kernel_test!(adjacent_pages_independent, TestKind::Physical, {
    let v0 = virt(0x0000_5000_0000_0000);
    let v1 = virt(0x0000_5000_0000_1000);
    let v2 = virt(0x0000_5000_0000_2000);

    let f0 = crate::mm::allocate();
    let f1 = crate::mm::allocate();
    let f2 = crate::mm::allocate();

    let p0 = f0.base();
    let p1 = f1.base();
    let p2 = f2.base();

    let mut mapper = Platform::new_test_mapper();

    do_map(&mut mapper, v0, f0, PageFlags::kernel_data()).flush();
    do_map(&mut mapper, v1, f1, PageFlags::kernel_code()).flush();
    do_map(&mut mapper, v2, f2, PageFlags::user_data()).flush();

    let (got_p0, flags0) = mapper.translate(v0).expect("translate v0 failed");
    let (got_p1, flags1) = mapper.translate(v1).expect("translate v1 failed");
    let (got_p2, flags2) = mapper.translate(v2).expect("translate v2 failed");

    kassert_eq!(got_p0.as_usize(), p0.as_usize());
    kassert_eq!(got_p1.as_usize(), p1.as_usize());
    kassert_eq!(got_p2.as_usize(), p2.as_usize());

    kassert!(flags0.contains(PageFlags::WRITE));
    kassert!(!flags0.contains(PageFlags::EXECUTE));

    kassert!(flags1.contains(PageFlags::EXECUTE));
    kassert!(!flags1.contains(PageFlags::WRITE));

    kassert!(flags2.contains(PageFlags::USER));
    kassert!(flags2.contains(PageFlags::WRITE));
    kassert!(!flags2.contains(PageFlags::EXECUTE));
});

kernel_test!(unmap_middle_page_leaves_neighbors, TestKind::Physical, {
    let v0 = virt(0x0000_0060_0000_0000);
    let v1 = virt(0x0000_0060_0000_1000);
    let v2 = virt(0x0000_0060_0000_2000);

    let f0 = crate::mm::allocate();
    let f1 = crate::mm::allocate();
    let f2 = crate::mm::allocate();

    let p0 = f0.base();
    let p2 = f2.base();

    let mut mapper = Platform::new_test_mapper();

    do_map(&mut mapper, v0, f0, PageFlags::kernel_data()).flush();
    do_map(&mut mapper, v1, f1, PageFlags::kernel_data()).flush();
    do_map(&mut mapper, v2, f2, PageFlags::kernel_data()).flush();

    let (returned, flush) = mapper.unmap(v1).expect("unmap v1 failed");
    flush.flush();
    crate::mm::deallocate(returned);

    kassert!(mapper.translate(v1).is_none());

    let (got_p0, _) = mapper.translate(v0).expect("v0 should still be mapped");
    let (got_p2, _) = mapper.translate(v2).expect("v2 should still be mapped");

    kassert_eq!(got_p0.as_usize(), p0.as_usize());
    kassert_eq!(got_p2.as_usize(), p2.as_usize());
});

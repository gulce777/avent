//! PMM subsystem tests.

use crate::mm;
use crate::mm::{Frame4K, FrameRange, PAGE_SIZE, PhysAddr, PhysFrame, VirtAddr};
use crate::test::{TestKind, TestResult};
use crate::{kassert, kassert_eq, kernel_test};

// PhysAddr Construction

kernel_test!(physaddr_new_zero, TestKind::Unit, {
    let addr = PhysAddr::new(0);
    kassert!(addr.is_some());
    kassert_eq!(addr.unwrap().as_usize(), 0);
});

kernel_test!(physaddr_new_valid, TestKind::Unit, {
    let addr = PhysAddr::new(0x0000_1234_5678_9000);
    kassert!(addr.is_some());
    kassert_eq!(addr.unwrap().as_usize(), 0x0000_1234_5678_9000);
});

kernel_test!(physaddr_new_at_max, TestKind::Unit, {
    let max = (1usize << 52) - 1;
    let addr = PhysAddr::new(max);
    kassert!(addr.is_some());
    kassert_eq!(addr.unwrap().as_usize(), max);
});

kernel_test!(physaddr_new_above_max, TestKind::Unit, {
    let over = 1usize << 52;
    kassert!(PhysAddr::new(over).is_none());
});

kernel_test!(physaddr_new_truncate_clears_high_bits, TestKind::Unit, {
    let raw = (1usize << 52) | 0x1000;
    let addr = PhysAddr::new_truncate(raw);
    kassert_eq!(addr.as_usize(), 0x1000);
});

kernel_test!(physaddr_new_truncate_valid_passthrough, TestKind::Unit, {
    let raw = 0x0000_DEAD_BEEF_0000usize;
    let addr = PhysAddr::new_truncate(raw);
    kassert_eq!(addr.as_usize(), raw);
});

kernel_test!(physaddr_max_constant, TestKind::Unit, {
    kassert_eq!(PhysAddr::MAX.as_usize(), (1usize << 52) - 1);
});

// PhysAddr alignment

kernel_test!(physaddr_is_aligned_page, TestKind::Unit, {
    let aligned = PhysAddr::new(0x4000).unwrap();
    kassert!(aligned.is_aligned(PAGE_SIZE));
});

kernel_test!(physaddr_is_aligned_not_page, TestKind::Unit, {
    let unaligned = PhysAddr::new(0x4001).unwrap();
    kassert!(!unaligned.is_aligned(PAGE_SIZE));
});

kernel_test!(physaddr_is_aligned_one, TestKind::Unit, {
    let addr = PhysAddr::new(0x1337).unwrap();
    kassert!(addr.is_aligned(1));
});

kernel_test!(physaddr_align_down_already_aligned, TestKind::Unit, {
    let addr = PhysAddr::new(0x8000).unwrap();
    kassert_eq!(addr.align_down(PAGE_SIZE).as_usize(), 0x8000);
});

kernel_test!(physaddr_align_down_mid_page, TestKind::Unit, {
    let addr = PhysAddr::new(0x8FFF).unwrap();
    kassert_eq!(addr.align_down(PAGE_SIZE).as_usize(), 0x8000);
});

kernel_test!(physaddr_align_down_one_below_boundary, TestKind::Unit, {
    let addr = PhysAddr::new(0x9000 - 1).unwrap();
    kassert_eq!(addr.align_down(PAGE_SIZE).as_usize(), 0x8000);
});

kernel_test!(physaddr_align_up_already_aligned, TestKind::Unit, {
    let addr = PhysAddr::new(0x8000).unwrap();
    kassert_eq!(addr.align_up(PAGE_SIZE).unwrap().as_usize(), 0x8000);
});

kernel_test!(physaddr_align_up_one_past_boundary, TestKind::Unit, {
    // One byte past a page boundary must round up to the next page.
    let addr = PhysAddr::new(0x8001).unwrap();
    kassert_eq!(addr.align_up(PAGE_SIZE).unwrap().as_usize(), 0x9000);
});

kernel_test!(physaddr_align_up_overflow_returns_none, TestKind::Unit, {
    let near_max = PhysAddr::MAX.align_down(PAGE_SIZE);

    if let Some(one_past) = near_max.checked_add(1) {
        kassert!(one_past.align_up(PAGE_SIZE).is_none());
    }
});

// PhysAddr arithmetic

kernel_test!(physaddr_checked_add_normal, TestKind::Unit, {
    let base = PhysAddr::new(0x1000).unwrap();
    let result = base.checked_add(0x2000).unwrap();
    kassert_eq!(result.as_usize(), 0x3000);
});

kernel_test!(physaddr_checked_add_to_max, TestKind::Unit, {
    let max = (1usize << 52) - 1;
    let base = PhysAddr::new(max - 0xFF).unwrap();
    let result = base.checked_add(0xFF).unwrap();
    kassert_eq!(result.as_usize(), max);
});

kernel_test!(physaddr_checked_add_overflow, TestKind::Unit, {
    let near = PhysAddr::new((1usize << 52) - 1).unwrap();
    kassert!(near.checked_add(1).is_none());
});

kernel_test!(physaddr_checked_sub_normal, TestKind::Unit, {
    let addr = PhysAddr::new(0x5000).unwrap();
    let result = addr.checked_sub(0x1000).unwrap();
    kassert_eq!(result.as_usize(), 0x4000);
});

kernel_test!(physaddr_checked_sub_to_zero, TestKind::Unit, {
    let addr = PhysAddr::new(0x1000).unwrap();
    let result = addr.checked_sub(0x1000).unwrap();
    kassert_eq!(result.as_usize(), 0);
});

kernel_test!(physaddr_checked_sub_underflow, TestKind::Unit, {
    let addr = PhysAddr::new(0).unwrap();
    kassert!(addr.checked_sub(1).is_none());
});

// VirtAddr - canonicality

kernel_test!(virtaddr_new_zero, TestKind::Unit, {
    kassert!(VirtAddr::new(0).is_some());
});

kernel_test!(virtaddr_new_low_half_max, TestKind::Unit, {
    let low_max = (1usize << 47) - 1;
    kassert!(VirtAddr::new(low_max).is_some());
});

kernel_test!(virtaddr_new_high_half_min, TestKind::Unit, {
    let high_min = !((1usize << 47) - 1);
    kassert!(VirtAddr::new(high_min).is_some());
});

kernel_test!(virtaddr_new_non_canonical, TestKind::Unit, {
    let non_canonical = 1usize << 47;
    kassert!(VirtAddr::new(non_canonical).is_none());
});

kernel_test!(virtaddr_new_canonical_sign_extends, TestKind::Unit, {
    let input = (1usize << 47) | 0x1000;
    let addr = VirtAddr::new_canonical(input);
    let high = addr.as_usize() >> 48;
    kassert_eq!(high, 0xFFFF);
});

kernel_test!(virtaddr_roundtrip, TestKind::Unit, {
    let raw = 0xFFFF_8000_0010_0000usize;
    let addr = VirtAddr::new(raw).unwrap();
    kassert_eq!(addr.as_usize(), raw);
});

// PhysFrame construction

kernel_test!(physframe_from_base_aligned, TestKind::Unit, {
    let base = PhysAddr::new(0x10_000).unwrap();
    kassert!(Frame4K::from_base(base).is_some());
});

kernel_test!(physframe_from_base_unaligned, TestKind::Unit, {
    let base = PhysAddr::new(0x10_001).unwrap();
    kassert!(Frame4K::from_base(base).is_none());
});

kernel_test!(physframe_from_base_zero, TestKind::Unit, {
    let base = PhysAddr::new(0).unwrap();
    kassert!(Frame4K::from_base(base).is_some());
});

kernel_test!(physframe_containing_aligned, TestKind::Unit, {
    let addr = PhysAddr::new(0x5000).unwrap();
    let frame = Frame4K::containing(addr);
    kassert_eq!(frame.base().as_usize(), 0x5000);
});

kernel_test!(physframe_containing_mid_page, TestKind::Unit, {
    let addr = PhysAddr::new(0x5FFE).unwrap();
    let frame = Frame4K::containing(addr);
    kassert_eq!(frame.base().as_usize(), 0x5000);
});

kernel_test!(physframe_containing_last_byte, TestKind::Unit, {
    let addr = PhysAddr::new(0x5FFF).unwrap();
    let frame = Frame4K::containing(addr);
    kassert_eq!(frame.base().as_usize(), 0x5000);
});

kernel_test!(physframe_containing_first_byte_next_page, TestKind::Unit, {
    let addr = PhysAddr::new(0x6000).unwrap();
    let frame = Frame4K::containing(addr);
    kassert_eq!(frame.base().as_usize(), 0x6000);
});

// PhysFrame index round-trip

kernel_test!(physframe_index_zero, TestKind::Unit, {
    let base = PhysAddr::new(0).unwrap();
    let frame = Frame4K::from_base(base).unwrap();
    kassert_eq!(frame.index(), 0);
});

kernel_test!(physframe_index_nonzero, TestKind::Unit, {
    let base = PhysAddr::new(0x5000).unwrap();
    let frame = Frame4K::from_base(base).unwrap();
    kassert_eq!(frame.index(), 5);
});

kernel_test!(physframe_from_index_roundtrip, TestKind::Unit, {
    for i in [0usize, 1, 7, 255, 1024, 0x10_0000] {
        let frame = Frame4K::from_index(i).unwrap();
        kassert_eq!(frame.index(), i);
    }
});

kernel_test!(physframe_index_from_base_roundtrip, TestKind::Unit, {
    let base = PhysAddr::new(0x42_000).unwrap();
    let frame = Frame4K::from_base(base).unwrap();
    let reconstructed = Frame4K::from_index(frame.index()).unwrap();
    kassert_eq!(reconstructed.base().as_usize(), base.as_usize());
});

// PhysFrame end address

kernel_test!(physframe_end_normal, TestKind::Unit, {
    let base = PhysAddr::new(0x1000).unwrap();
    let frame = Frame4K::from_base(base).unwrap();
    kassert_eq!(frame.end().unwrap().as_usize(), 0x2000);
});

kernel_test!(physframe_end_frame_zero, TestKind::Unit, {
    let frame = Frame4K::from_index(0).unwrap();
    kassert_eq!(frame.end().unwrap().as_usize(), PAGE_SIZE);
});

// FrameRange

kernel_test!(framerange_len_single, TestKind::Unit, {
    let f = Frame4K::from_index(5).unwrap();
    let range = FrameRange::new(f, f);
    kassert_eq!(range.len(), 1);
});

kernel_test!(framerange_len_multiple, TestKind::Unit, {
    let start = Frame4K::from_index(3).unwrap();
    let end = Frame4K::from_index(7).unwrap();
    let range = FrameRange::new(start, end);
    kassert_eq!(range.len(), 5);
});

kernel_test!(framerange_is_empty_false, TestKind::Unit, {
    let start = Frame4K::from_index(1).unwrap();
    let end = Frame4K::from_index(3).unwrap();
    kassert!(!FrameRange::new(start, end).is_empty());
});

kernel_test!(framerange_is_empty_true, TestKind::Unit, {
    let start = Frame4K::from_index(5).unwrap();
    let end = Frame4K::from_index(3).unwrap();
    kassert!(FrameRange::new(start, end).is_empty());
});

kernel_test!(framerange_contains_start, TestKind::Unit, {
    let start = Frame4K::from_index(10).unwrap();
    let end = Frame4K::from_index(20).unwrap();
    let range = FrameRange::new(start, end);
    kassert!(range.contains(start));
});

kernel_test!(framerange_contains_end, TestKind::Unit, {
    let start = Frame4K::from_index(10).unwrap();
    let end = Frame4K::from_index(20).unwrap();
    let range = FrameRange::new(start, end);
    kassert!(range.contains(end));
});

kernel_test!(framerange_contains_mid, TestKind::Unit, {
    let start = Frame4K::from_index(10).unwrap();
    let end = Frame4K::from_index(20).unwrap();
    let mid = Frame4K::from_index(15).unwrap();
    let range = FrameRange::new(start, end);
    kassert!(range.contains(mid));
});

kernel_test!(framerange_not_contains_before, TestKind::Unit, {
    let start = Frame4K::from_index(10).unwrap();
    let end = Frame4K::from_index(20).unwrap();
    let before = Frame4K::from_index(9).unwrap();
    let range = FrameRange::new(start, end);
    kassert!(!range.contains(before));
});

kernel_test!(framerange_not_contains_after, TestKind::Unit, {
    let start = Frame4K::from_index(10).unwrap();
    let end = Frame4K::from_index(20).unwrap();
    let after = Frame4K::from_index(21).unwrap();
    let range = FrameRange::new(start, end);
    kassert!(!range.contains(after));
});

kernel_test!(framerange_iterator_count, TestKind::Unit, {
    let start = Frame4K::from_index(0).unwrap();
    let end = Frame4K::from_index(3).unwrap();
    let range = FrameRange::new(start, end);
    kassert_eq!(range.count(), 4);
});

kernel_test!(framerange_iterator_order, TestKind::Unit, {
    let start = Frame4K::from_index(7).unwrap();
    let end = Frame4K::from_index(9).unwrap();
    let mut range = FrameRange::new(start, end);
    kassert_eq!(range.next().unwrap().index(), 7);
    kassert_eq!(range.next().unwrap().index(), 8);
    kassert_eq!(range.next().unwrap().index(), 9);
    kassert!(range.next().is_none());
});

kernel_test!(framerange_iterator_exhausted, TestKind::Unit, {
    let f = Frame4K::from_index(0).unwrap();
    let mut range = FrameRange::new(f, f);
    let _ = range.next();
    kassert!(range.next().is_none());
    kassert!(range.next().is_none());
});

kernel_test!(framerange_from_addr_len_basic, TestKind::Unit, {
    let base = PhysAddr::new(0x2000).unwrap();
    let range = FrameRange::<{ PAGE_SIZE }>::from_addr_len(base, PAGE_SIZE * 4).unwrap();
    kassert_eq!(range.start().index(), 2);
    kassert_eq!(range.end().index(), 5);
});

kernel_test!(framerange_from_addr_len_zero, TestKind::Unit, {
    let base = PhysAddr::new(0x1000).unwrap();
    kassert!(FrameRange::<{ PAGE_SIZE }>::from_addr_len(base, 0).is_none());
});

// Global allocator basic correctness

kernel_test!(alloc_returns_page_aligned_frame, TestKind::Physical, {
    let frame = mm::allocate();
    let aligned = frame.base().as_usize() % PAGE_SIZE == 0;
    mm::deallocate(frame);
    kassert!(aligned);
});

kernel_test!(alloc_base_within_52_bits, TestKind::Physical, {
    let frame = mm::allocate();
    let in_range = frame.base().as_usize() <= (1usize << 52) - 1;
    mm::deallocate(frame);
    kassert!(in_range);
});

kernel_test!(total_frames_nonzero, TestKind::Physical, {
    kassert!(mm::total_frames() > 0);
});

kernel_test!(free_le_total, TestKind::Physical, {
    kassert!(mm::free_frames() <= mm::total_frames());
});

// Global allocator counter invariants

kernel_test!(free_count_decreases_by_one_on_alloc, TestKind::Physical, {
    let before = mm::free_frames();
    let frame = mm::allocate();
    let after = mm::free_frames();
    mm::deallocate(frame);
    kassert_eq!(before - 1, after);
});

kernel_test!(free_count_restores_after_dealloc, TestKind::Physical, {
    let before = mm::free_frames();
    let frame = mm::allocate();
    mm::deallocate(frame);
    kassert_eq!(mm::free_frames(), before);
});

kernel_test!(total_stable_across_alloc_dealloc, TestKind::Physical, {
    let total = mm::total_frames();
    let frame = mm::allocate();
    kassert_eq!(mm::total_frames(), total);
    mm::deallocate(frame);
    kassert_eq!(mm::total_frames(), total);
});

kernel_test!(multi_alloc_decreases_count_by_n, TestKind::Physical, {
    let before = mm::free_frames();
    let f0 = mm::allocate();
    let f1 = mm::allocate();
    let f2 = mm::allocate();
    kassert_eq!(mm::free_frames(), before - 3);
    mm::deallocate(f0);
    mm::deallocate(f1);
    mm::deallocate(f2);
    kassert_eq!(mm::free_frames(), before);
});

// Global allocator aliasing

kernel_test!(two_live_allocs_never_alias, TestKind::Physical, {
    let a = mm::allocate();
    let b = mm::allocate();
    let differ = a.base().as_usize() != b.base().as_usize();
    mm::deallocate(a);
    mm::deallocate(b);
    kassert!(differ);
});

kernel_test!(three_live_allocs_all_distinct, TestKind::Physical, {
    let a = mm::allocate();
    let b = mm::allocate();
    let c = mm::allocate();

    let ba = a.base().as_usize();
    let bb = b.base().as_usize();
    let bc = c.base().as_usize();

    mm::deallocate(a);
    mm::deallocate(b);
    mm::deallocate(c);

    kassert!(ba != bb);
    kassert!(bb != bc);
    kassert!(ba != bc);
});

// Global allocator LIFO free list ordering

kernel_test!(lifo_single_frame, TestKind::Physical, {
    let frame = mm::allocate();
    let base = frame.base().as_usize();
    mm::deallocate(frame);
    let again = mm::allocate();
    let same = again.base().as_usize() == base;
    mm::deallocate(again);
    kassert!(same);
});

kernel_test!(lifo_three_frame_chain, TestKind::Physical, {
    let a = mm::allocate();
    let b = mm::allocate();
    let c = mm::allocate();

    let base_a = a.base().as_usize();
    let base_b = b.base().as_usize();
    let base_c = c.base().as_usize();

    mm::deallocate(a);
    mm::deallocate(b);
    mm::deallocate(c);

    let c2 = mm::allocate();
    let b2 = mm::allocate();
    let a2 = mm::allocate();

    let ok_c = c2.base().as_usize() == base_c;
    let ok_b = b2.base().as_usize() == base_b;
    let ok_a = a2.base().as_usize() == base_a;

    mm::deallocate(c2);
    mm::deallocate(b2);
    mm::deallocate(a2);

    kassert!(ok_c);
    kassert!(ok_b);
    kassert!(ok_a);
});

kernel_test!(lifo_interleaved, TestKind::Physical, {
    let a = mm::allocate();
    let b = mm::allocate();
    let base_b = b.base().as_usize();

    mm::deallocate(b);

    let b2 = mm::allocate();
    let got_b = b2.base().as_usize() == base_b;

    mm::deallocate(a);
    mm::deallocate(b2);

    kassert!(got_b);
});

// Global allocator stress

kernel_test!(
    stress_sequential_alloc_dealloc_no_leak,
    TestKind::Physical,
    {
        const N: usize = 64;
        let baseline = mm::free_frames();
        for _ in 0..N {
            let frame = mm::allocate();
            mm::deallocate(frame);
        }
        kassert_eq!(mm::free_frames(), baseline);
    }
);

kernel_test!(stress_bulk_alloc_then_free, TestKind::Physical, {
    const N: usize = 16;
    let baseline = mm::free_frames();

    let mut frames: [core::mem::MaybeUninit<crate::mm::OwnedFrame>; N] =
        core::array::from_fn(|_| core::mem::MaybeUninit::uninit());
    let mut bases = [0usize; N];

    for i in 0..N {
        let f = mm::allocate();
        bases[i] = f.base().as_usize();
        frames[i].write(f);
    }

    for i in 0..N {
        kassert_eq!(bases[i] % PAGE_SIZE, 0);
        for j in (i + 1)..N {
            kassert!(bases[i] != bases[j]);
        }
    }

    kassert_eq!(mm::free_frames(), baseline - N);

    // Free in reverse order to exercise the LIFO chain thoroughly.
    for i in (0..N).rev() {
        // SAFETY: every slot was written in the allocation loop above.
        let f = unsafe { frames[i].assume_init_read() };
        mm::deallocate(f);
    }

    kassert_eq!(mm::free_frames(), baseline);
});

kernel_test!(stress_free_never_exceeds_total, TestKind::Physical, {
    const N: usize = 8;
    let total = mm::total_frames();

    let mut frames: [core::mem::MaybeUninit<crate::mm::OwnedFrame>; N] =
        core::array::from_fn(|_| core::mem::MaybeUninit::uninit());

    for i in 0..N {
        kassert!(mm::free_frames() <= total);
        frames[i].write(mm::allocate());
    }

    for i in 0..N {
        // SAFETY: slot i was written in the loop above.
        let f = unsafe { frames[i].assume_init_read() };
        mm::deallocate(f);
        kassert!(mm::free_frames() <= total);
    }
});

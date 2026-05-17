use crate::mm::heap::{BLOCK_ALIGN, HeapAllocator, MIN_BLOCK_SIZE};
use crate::test::{TestKind, TestResult};
use crate::{kassert, kassert_eq, kernel_test};

#[repr(C, align(16))]
struct AlignedBuf<const N: usize>([u8; N]);

impl<const N: usize> AlignedBuf<N> {
    const fn new() -> Self {
        Self([0u8; N])
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.0.as_mut_ptr()
    }

    fn len(&self) -> usize {
        N
    }
}

kernel_test!(heap_new_is_empty, TestKind::Unit, {
    let allocator = HeapAllocator::new();
    kassert!(allocator.is_empty());
    kassert_eq!(allocator.free_bytes(), 0);
    kassert_eq!(allocator.total_bytes(), 0);
    kassert_eq!(allocator.free_list_len(), 0);
});

kernel_test!(heap_add_memory_single_region, TestKind::Unit, {
    let mut buf = AlignedBuf::<1024>::new();
    let mut allocator = HeapAllocator::new();

    unsafe { allocator.add_memory(buf.as_mut_ptr(), buf.len()) };

    let expected = buf.len() & !(BLOCK_ALIGN - 1);
    kassert_eq!(allocator.total_bytes(), expected);
    kassert_eq!(allocator.free_bytes(), expected);
    kassert_eq!(allocator.free_list_len(), 1);
});

kernel_test!(heap_add_memory_rounds_size, TestKind::Unit, {
    let mut buf = AlignedBuf::<1023>::new();
    let mut allocator = HeapAllocator::new();

    unsafe { allocator.add_memory(buf.as_mut_ptr(), buf.len()) };

    let expected = (buf.len() / BLOCK_ALIGN) * BLOCK_ALIGN;
    kassert_eq!(allocator.total_bytes(), expected);
    kassert_eq!(allocator.free_bytes(), expected);
});

kernel_test!(heap_add_memory_two_non_adjacent, TestKind::Unit, {
    let mut buf = AlignedBuf::<1536>::new();
    let ptr = buf.as_mut_ptr();

    let chunk_size = 512;
    let gap_size = 512;

    let mut allocator = HeapAllocator::new();

    unsafe { allocator.add_memory(ptr, chunk_size) };

    unsafe { allocator.add_memory(ptr.add(chunk_size + gap_size), chunk_size) };

    let expected = chunk_size * 2;
    kassert_eq!(allocator.total_bytes(), expected);
    kassert_eq!(allocator.free_bytes(), expected);

    kassert_eq!(allocator.free_list_len(), 2);
});

kernel_test!(heap_add_memory_two_adjacent_merge, TestKind::Unit, {
    let mut buf = AlignedBuf::<1024>::new();
    let ptr = buf.as_mut_ptr();
    let half = buf.len() / 2;

    let mut allocator = HeapAllocator::new();

    unsafe { allocator.add_memory(ptr, half) };
    unsafe { allocator.add_memory(ptr.add(half), half) };

    kassert_eq!(allocator.total_bytes(), buf.len());
    kassert_eq!(allocator.free_bytes(), buf.len());
    kassert_eq!(allocator.free_list_len(), 1);
});

kernel_test!(heap_add_memory_reverse_order_merge, TestKind::Unit, {
    let mut buf = AlignedBuf::<1024>::new();
    let ptr = buf.as_mut_ptr();
    let half = buf.len() / 2;

    let mut allocator = HeapAllocator::new();

    unsafe { allocator.add_memory(ptr.add(half), half) };
    unsafe { allocator.add_memory(ptr, half) };

    kassert_eq!(allocator.total_bytes(), buf.len());
    kassert_eq!(allocator.free_bytes(), buf.len());
    kassert_eq!(allocator.free_list_len(), 1);
});

kernel_test!(heap_stats_after_add_memory, TestKind::Unit, {
    let mut buf = AlignedBuf::<2048>::new();
    let mut allocator = HeapAllocator::new();

    unsafe { allocator.add_memory(buf.as_mut_ptr(), buf.len()) };

    let stats = allocator.stats();
    kassert_eq!(stats.total_bytes, buf.len());
    kassert_eq!(stats.free_bytes, buf.len());
    kassert_eq!(stats.alloc_count, 0);
    kassert_eq!(stats.dealloc_count, 0);
    kassert!(stats.is_balanced());
});

kernel_test!(heap_alloc_dealloc_simple, TestKind::Unit, {
    let mut buf = AlignedBuf::<1024>::new();
    let mut allocator = HeapAllocator::new();
    unsafe { allocator.add_memory(buf.as_mut_ptr(), buf.len()) };

    let size = 32;
    let align = 16;

    let ptr = allocator.alloc(size, align).expect("alloc should not fail");

    kassert_eq!((ptr.as_ptr() as usize) % align, 0);

    let stats_after_alloc = allocator.stats();
    kassert_eq!(stats_after_alloc.alloc_count, 1);
    kassert_eq!(stats_after_alloc.dealloc_count, 0);
    kassert!(stats_after_alloc.free_bytes < 1024);

    unsafe { allocator.dealloc(ptr, size, align) };

    kassert!(allocator.stats().is_balanced());
    kassert_eq!(allocator.free_bytes(), 1024);
    kassert_eq!(allocator.free_list_len(), 1);
});

kernel_test!(heap_alloc_exact_exhaustion, TestKind::Unit, {
    let mut buf = AlignedBuf::<128>::new();
    let mut allocator = HeapAllocator::new();
    unsafe { allocator.add_memory(buf.as_mut_ptr(), buf.len()) };

    let ptr = allocator.alloc(128, 16).expect("alloc should not fail");

    kassert_eq!(allocator.free_bytes(), 0);
    kassert!(allocator.is_empty());
    kassert_eq!(allocator.free_list_len(), 0);

    unsafe { allocator.dealloc(ptr, 128, 16) };

    kassert!(allocator.stats().is_balanced());
    kassert_eq!(allocator.free_list_len(), 1);
});

kernel_test!(heap_alloc_oom, TestKind::Unit, {
    let mut buf = AlignedBuf::<128>::new();
    let mut allocator = HeapAllocator::new();
    unsafe { allocator.add_memory(buf.as_mut_ptr(), buf.len()) };

    let ptr = allocator.alloc(256, 16);

    kassert!(ptr.is_none());
    kassert_eq!(allocator.stats().alloc_count, 0);
    kassert_eq!(allocator.free_bytes(), 128);
});

kernel_test!(heap_alloc_multiple_and_coalesce, TestKind::Unit, {
    let mut buf = AlignedBuf::<1024>::new();
    let mut allocator = HeapAllocator::new();
    unsafe { allocator.add_memory(buf.as_mut_ptr(), buf.len()) };

    let size = 64;
    let align = 16;

    let ptr_a = allocator.alloc(size, align).unwrap();
    let ptr_b = allocator.alloc(size, align).unwrap();
    let ptr_c = allocator.alloc(size, align).unwrap();

    kassert_eq!(allocator.free_list_len(), 1);

    unsafe { allocator.dealloc(ptr_b, size, align) };
    kassert_eq!(allocator.free_list_len(), 2);

    unsafe { allocator.dealloc(ptr_a, size, align) };
    kassert_eq!(allocator.free_list_len(), 2);

    unsafe { allocator.dealloc(ptr_c, size, align) };

    kassert!(allocator.stats().is_balanced());
    kassert_eq!(allocator.free_list_len(), 1);
    kassert_eq!(allocator.free_bytes(), 1024);
});

kernel_test!(heap_alloc_high_alignment, TestKind::Unit, {
    let mut buf = AlignedBuf::<2048>::new();
    let mut allocator = HeapAllocator::new();
    unsafe { allocator.add_memory(buf.as_mut_ptr(), buf.len()) };

    let size = 32;
    let align = 128;

    let ptr1 = allocator.alloc(size, align).expect("alloc 1 failed");
    let ptr2 = allocator.alloc(size, align).expect("alloc 2 failed");

    kassert_eq!((ptr1.as_ptr() as usize) % align, 0);
    kassert_eq!((ptr2.as_ptr() as usize) % align, 0);

    unsafe {
        allocator.dealloc(ptr1, size, align);
        allocator.dealloc(ptr2, size, align);
    }

    kassert!(allocator.stats().is_balanced());
    kassert_eq!(allocator.free_list_len(), 1);
});

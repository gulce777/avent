use super::stack::{DEFAULT_STACK_SIZE, KernelStack, STACK_ALIGN};
use crate::test::{TestKind, TestResult};
use crate::{kassert, kassert_eq, kernel_test};

kernel_test!(kernel_stack_size, TestKind::Unit, {
    let stack = KernelStack::new(4096);
    kassert_eq!(stack.size(), 4096);
});

kernel_test!(kernel_stack_top_within_buf, TestKind::Unit, {
    let stack = KernelStack::new(4096);
    let base = stack.base() as usize;
    let top = stack.top() as usize;
    kassert!(top > base);
    kassert!(top <= base + stack.size());
});

kernel_test!(kernel_stack_top_aligned, TestKind::Unit, {
    let stack = KernelStack::new(4096);
    kassert!(stack.top() as usize % STACK_ALIGN == 0);
});

kernel_test!(kernel_stack_top_aligned_odd_size, TestKind::Unit, {
    let stack = KernelStack::new(4097);
    kassert!(stack.top() as usize % STACK_ALIGN == 0);
});

kernel_test!(kernel_stack_no_alias, TestKind::Unit, {
    let a = KernelStack::new(4096);
    let b = KernelStack::new(4096);
    kassert!(a.base() != b.base());
});

kernel_test!(kernel_stack_default_size, TestKind::Unit, {
    let stack = KernelStack::new_default();
    kassert_eq!(stack.size(), DEFAULT_STACK_SIZE);
});

kernel_test!(kernel_stack_zeroed, TestKind::Unit, {
    let stack = KernelStack::new(4096);
    let slice = unsafe { core::slice::from_raw_parts(stack.base(), stack.size()) };
    kassert!(slice.iter().all(|&b| b == 0));
});

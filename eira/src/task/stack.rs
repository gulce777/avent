//! Kernel stack allocation for tasks.
//!
//! A [`KernelStack`] is a fixed-size, heap-backed buffer used as the kernel stack
//! for a single task. It is allocated once when the task is created and freed automatically
//! when the task is dropped.

use alloc::boxed::Box;
use alloc::vec;

/// Default kernel stack size: 64 KiB.
///
/// Large enough for deeply nested kernel calls and interrupt handlers that
/// may re-enter the kernel during a task switch. Adjust if profiling shows
/// stack pressure.
pub const DEFAULT_STACK_SIZE: usize = 64 * 1024; // 64 KiB

/// Required alignment of the initial stack pointer.
///
/// Both the System V AMD64 ABI and the AArch64 PCS require the stack pointer
/// to be 16-byte aligned before a `call` / `bl` instruction. We enforce this
/// on the initial stack pointer so the first function call from a fresh task
/// is always correctly aligned.
pub(super) const STACK_ALIGN: usize = 16;

/// A heap-backed kernel stack.
///
/// Owns the backing memory for the duration of the task's lifetime. When
/// dropped, the memory is returned to the kernel heap automatically.
pub struct KernelStack {
    /// The backing buffer. Stored in a `Box<[u8]>` so ownership and
    /// deallocation are handled by Rust's allocator automatically.
    buf: Box<[u8]>,
}

impl KernelStack {
    /// Allocate a new kernel stack of `size` bytes.
    ///
    /// The backing memory is zeroed on allocation so that uninitialized stack
    /// reads produce deterministic (if wrong) behaviour rather than undefined
    /// behaviour in debug builds.
    ///
    /// # Panics
    ///
    /// - `size` is zero.
    /// - `size` is less than [`STACK_ALIGN`].
    /// - The kernel heap is exhausted.
    pub fn new(size: usize) -> Self {
        assert!(
            size >= STACK_ALIGN,
            "stack size must be at least {STACK_ALIGN}"
        );

        let buf = vec![0u8; size].into_boxed_slice();

        Self { buf }
    }

    /// Allocate a new kernel stack with the [`DEFAULT_STACK_SIZE`].
    #[inline]
    pub fn new_default() -> Self {
        Self::new(DEFAULT_STACK_SIZE)
    }

    /// Returns the size of the stack buffer in bytes.
    #[inline]
    pub fn size(&self) -> usize {
        self.buf.len()
    }

    /// Returns a pointer to the lowest byte of the stack buffer.
    ///
    /// This is the *base* of the allocation, not the initial stack pointer.
    /// Use [`top`](Self::top) to get the value to load into RSP / SP.
    #[inline]
    pub fn base(&self) -> *const u8 {
        self.buf.as_ptr()
    }

    /// Returns the initial stack pointer for this stack.
    ///
    /// The returned address is the highest address in the buffer, aligned
    /// down to [`STACK_ALIGN`] bytes. This is the value that should be
    /// loaded into RSP (x86_64) or SP (aarch64) when the task first runs.
    #[inline]
    pub fn top(&self) -> *mut u8 {
        let end = self.buf.as_ptr() as usize + self.buf.len();
        let aligned = end & !(STACK_ALIGN - 1);
        aligned as *mut u8
    }
}

impl core::fmt::Debug for KernelStack {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("KernelStack")
            .field("base", &self.base())
            .field("top", &self.top())
            .field("size", &self.size())
            .finish()
    }
}

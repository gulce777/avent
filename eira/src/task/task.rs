//! Kernel task representation.

use alloc::sync::Arc;

use spin::Mutex;

use crate::mm::KernelAddressSpace;
use crate::task::context::{KernelContext, TaskContext};
use crate::task::stack::KernelStack;

/// A globally unique, monotonically increasing task identifier.
///
/// IDs are never reused within one kernel session. `0` is reserved for the
/// per-CPU idle task; all real tasks start at `1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(u64);

impl TaskId {
    pub const IDLE: Self = Self(0);

    pub fn next() -> Self {
        use core::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

impl core::fmt::Display for TaskId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "task#{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// Newly created; not yet on any run queue.
    Created,
    /// On the run queue, waiting for CPU time.
    Ready,
    /// Currently executing on a CPU.
    Running,
    /// Waiting for an external event.
    Blocked(BlockReason),
    /// Exited or killed; awaiting reaper cleanup.
    Dead,
}

/// The reason a task is currently blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockReason {
    /// Waiting for an IPC reply.
    IpcReply,
    /// Waiting for a timer to fire.
    Sleep,
    /// Waiting for a VFS / I/O operation.
    Io,
    /// Waiting for a page fault to be resolved by a pager.
    PageFault,
    /// Driver- or arch-specific reason.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(u8);

impl Priority {
    pub const IDLE: Self = Self(0);
    pub const NORMAL: Self = Self(128);
    pub const HIGH: Self = Self(200);
    pub const MAX: Self = Self(255);

    #[inline]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    #[inline]
    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

impl Default for Priority {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// A schedulable kernel task.
///
/// Always heap-allocated and owned by the scheduler. Never moved after [`KernelContext`]
/// is initialised. The context stores a raw stack pointer that would become dangling on a move.
pub struct Task {
    /// Globally unique identifier.
    pub id: TaskId,

    /// Current scheduling state.
    pub state: TaskState,

    /// Scheduling priority.
    pub priority: Priority,

    /// Saved CPU context.
    ///
    /// Uninitialised until [`Task::new`] calls [`KernelContext::new`].
    /// After that, only [`switch_to`](`KernelContext::switch_to`) may mutate it.
    pub context: KernelContext,

    /// Kernel-mode execution stack.
    ///
    /// Kept alive here so it is freed exactly when the task is dropped.
    pub stack: KernelStack,

    /// The virtual address space this task executes in.
    ///
    /// Shared (via `Arc`) with any sibling threads in the same process.
    /// Kernel tasks all share the single kernel address space.
    pub address_space: Arc<Mutex<KernelAddressSpace>>,
}

impl Task {
    /// Create a new task that will begin execution at `entry`.
    ///
    /// The task is left in [`TaskState::Created`]. The scheduler is responsible for
    /// transitioning it to [`TaskState::Ready`] and placing it on the run queue.
    ///
    /// # Panics
    ///
    /// - The kernel heap is exhausted.
    pub fn new(
        entry: fn() -> !,
        priority: Priority,
        stack_size: usize,
        address_space: Arc<Mutex<KernelAddressSpace>>,
    ) -> Self {
        let stack = KernelStack::new(stack_size);

        // SAFETY:
        // - `stack.top()` is 16-byte aligned (enforced by KernelStack).
        // - The stack buffer is heap-allocated and lives as long as `stack`,
        //   which is owned by this Task.
        let context = unsafe { KernelContext::new(entry, stack.top()) };

        Self {
            id: TaskId::next(),
            state: TaskState::Created,
            priority,
            context,
            stack,
            address_space,
        }
    }

    /// Create a kernel thread with the default stack size and normal priority.
    ///
    /// Convenience wrapper around [`Task::new`].
    pub fn new_kernel(entry: fn() -> !, address_space: Arc<Mutex<KernelAddressSpace>>) -> Self {
        Self::new(
            entry,
            Priority::NORMAL,
            crate::task::stack::DEFAULT_STACK_SIZE,
            address_space,
        )
    }

    /// Returns `true` if this task can be selected by the scheduler.
    #[inline]
    pub fn is_runnable(&self) -> bool {
        matches!(self.state, TaskState::Ready)
    }

    /// Transition the task to [`TaskState::Ready`].
    ///
    /// Valid from `Created` or `Blocked`. Panics in debug builds if called on
    /// a `Running` or `Dead` task.
    #[inline]
    pub fn mark_ready(&mut self) {
        debug_assert!(
            !matches!(self.state, TaskState::Running | TaskState::Dead),
            "mark_ready called on {:?} task {}",
            self.state,
            self.id,
        );
        self.state = TaskState::Ready;
    }

    /// Transition the task to [`TaskState::Blocked`] with the given reason.
    ///
    /// Must only be called on a `Running` task.
    #[inline]
    pub fn block(&mut self, reason: BlockReason) {
        debug_assert_eq!(
            self.state,
            TaskState::Running,
            "block() called on non-running task {}",
            self.id,
        );
        self.state = TaskState::Blocked(reason);
    }

    /// Transition the task to [`TaskState::Dead`].
    #[inline]
    pub fn kill(&mut self) {
        self.state = TaskState::Dead;
    }
}

impl core::fmt::Debug for Task {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Task")
            .field("id", &self.id)
            .field("state", &self.state)
            .field("priority", &self.priority)
            .finish()
    }
}

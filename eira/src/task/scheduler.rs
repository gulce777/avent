//! Cooperative kernel task scheduler.
//!
//! # Algorithm
//!
//! Priority-aware round-robin. [`schedule`] always picks the highest-priority
//! [`Ready`](TaskState::Ready) task from the run queue. Tasks with equal priority
//! are served in FIFO order. Preemption is not yet implemented, tasks must
//! voluntarily call [`yield_now`] or block to relinquish the CPU.
//!
//! # Global state
//!
//! One [`Scheduler`] instance lives behind a [`spin::Mutex`] in [`SCHEDULER`]. All public
//! entry points lock that mutex, perform their operation and release it before the actual
//! context switch so the switch happens with no lock held.
//!
//! # Idle task
//!
//! If the run queue is empty when [`schedule`] is called, the scheduler resumes the idle task [`TaskId::IDLE`]
//! which spins on `hlt`. The idle task is created automatically during [`init`] and never appears on the
//! run queue itself.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;

use spin::Mutex;

use crate::arch::{Arch, Platform};
use crate::mm::address_space::KernelAddressSpace;
use crate::task::context::{KernelContext, TaskContext};
use crate::task::stack::DEFAULT_STACK_SIZE;
use crate::task::task::{BlockReason, Priority, Task, TaskId, TaskState};

pub static SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);

/// The kernel task scheduler.
pub struct Scheduler {
    /// Tasks that are ready to run, in insertion order within each priority.
    run_queue: VecDeque<Box<Task>>,

    /// The task currently executing on this CPU.
    ///
    /// `None` only during the very first [`schedule`] call, before any task
    /// has been selected. After `init` completes this is always `Some`.
    current: Option<Box<Task>>,

    /// The idle task. Runs when `run_queue` is empty.
    ///
    /// Never placed on the run queue; held here exclusively.
    idle: Box<Task>,

    /// Tasks that have called [`exit`] and are waiting for their stack and
    /// address-space reference to be dropped.
    ///
    /// The reaper drains this on every [`schedule`] call.
    dead: VecDeque<Box<Task>>,
}

impl Scheduler {
    /// Construct a scheduler with a pre-built idle task.
    fn new(idle: Box<Task>) -> Self {
        Self {
            run_queue: VecDeque::new(),
            current: None,
            idle,
            dead: VecDeque::new(),
        }
    }

    /// Add a task to the run queue.
    ///
    /// Tasks are not sorted on insertion. [`pick_next`](Self::pick_next)
    /// performs a linear scan to find the highest-priority ready task. This
    /// keeps insertion O(1) at the cost of O(n) scheduling. Acceptable for
    /// a kernel with O(10–100) tasks.
    fn enqueue(&mut self, task: Box<Task>) {
        let mut task = task;
        task.mark_ready();
        self.run_queue.push_back(task);
    }

    /// Remove and return the highest-priority [`Ready`](TaskState::Ready) task
    /// from the run queue.
    ///
    /// Among tasks with equal priority the one closest to the front (oldest
    /// insertion) is chosen, preserving FIFO fairness.
    ///
    /// Returns `None` if the run queue contains no ready tasks.
    fn pick_next(&mut self) -> Option<Box<Task>> {
        let mut best_idx = None;
        let mut best_priority = 0u8;

        for (i, t) in self.run_queue.iter().enumerate() {
            if t.is_runnable() && t.priority.as_u8() > best_priority {
                best_priority = t.priority.as_u8();
                best_idx = Some(i);
            }
        }

        best_idx.and_then(|i| self.run_queue.remove(i))
    }

    /// Drop all tasks sitting in the dead queue.
    ///
    /// Called at the start of every [`schedule`] so stacks and address-space
    /// `Arc` references are freed promptly.
    fn reap(&mut self) {
        self.dead.clear();
    }

    /// Total number of tasks known to the scheduler (ready + current + dead).
    #[allow(dead_code)]
    pub fn task_count(&self) -> usize {
        self.run_queue.len() + usize::from(self.current.is_some()) + self.dead.len()
    }
}

/// Initialise the global scheduler and create the idle task.
///
/// Must be called exactly once, after the heap and frame allocator are ready,
/// before any call to [`spawn`] or [`yield_now`].
///
/// # Panics
///
/// Called more than once.
pub fn init(kernel_address_space: Arc<Mutex<KernelAddressSpace>>) {
    let mut guard = SCHEDULER.lock();
    assert!(guard.is_none(), "scheduler::init() called more than once");

    let idle = Box::new(Task::new(
        idle_task,
        Priority::IDLE,
        DEFAULT_STACK_SIZE,
        kernel_address_space,
    ));

    *guard = Some(Scheduler::new(idle));

    log::debug!("scheduler initialised");
}

/// Spawn a new kernel task and place it on the run queue.
///
/// # Parameters
///
/// - `entry`         — entry point; must never return.
/// - `priority`      — scheduling priority.
/// - `address_space` — address space the task runs in.
///
/// # Returns
///
/// The [`TaskId`] of the newly created task.
///
/// # Panics
///
/// The scheduler has not been initialised.
pub fn spawn(
    entry: fn() -> !,
    priority: Priority,
    address_space: Arc<Mutex<KernelAddressSpace>>,
) -> TaskId {
    let task = Box::new(Task::new(
        entry,
        priority,
        DEFAULT_STACK_SIZE,
        address_space,
    ));
    let id = task.id;

    with_scheduler(|s| s.enqueue(task));

    log::debug!("spawned {id} (priority={})", priority.as_u8());
    id
}

/// Voluntarily yield the CPU to the next ready task.
///
/// If no other task is ready the current task continues running immediately.
///
/// # Panics
///
/// The scheduler has not been initialised.
pub fn yield_now() {
    #[inline(never)]
    fn inner() {
        schedule(None);
    }
    inner();
}

/// Block the current task until [`unblock`] is called.
///
/// The task is removed from the run queue and will not be scheduled again
/// until some other code calls [`unblock`] with this task's [`TaskId`].
///
/// # Panics
///
/// - The scheduler has not been initialised.
/// - Called when no current task exists (before first `schedule`).
pub fn block(reason: BlockReason) {
    schedule(Some(reason));
}

/// Unblock a task that previously called [`block`].
///
/// Moves the task from wherever it is waiting back onto the run queue.
///
/// # Panics
///
/// - The scheduler has not been initialised.
/// - `id` does not refer to a blocked task.
pub fn unblock(id: TaskId) {
    with_scheduler(|s| {
        // Search the run_queue (task might have been enqueued but not yet
        // picked, or it may already be in a blocked state there).
        if let Some(task) = s.run_queue.iter_mut().find(|t| t.id == id) {
            task.mark_ready();
            return;
        }

        // Search the dead queue — should not happen, but guard anyway.
        if s.dead.iter().any(|t| t.id == id) {
            log::warn!("unblock({id}): task is dead, ignoring");
            return;
        }

        panic!("unblock({id}): task not found");
    });
}

/// Terminate the current task.
///
/// Moves it to the dead queue and immediately schedules the next task.
/// Never returns.
///
/// # Panics
///
/// The scheduler has not been initialised.
pub fn exit() -> ! {
    // Mark dead and schedule away. The dead queue is reaped next tick.
    with_scheduler(|s| {
        if let Some(mut task) = s.current.take() {
            task.kill();
            s.dead.push_back(task);
        }
    });

    // schedule() with no current task will pick the next ready task and
    // switch into it. We pass a sentinel block-reason that is never observed
    // because the task is already moved to the dead queue above.
    schedule(None);

    unreachable!("exit() returned from schedule()");
}

/// Core scheduling routine.
///
/// Performs the context switch with no lock held.
#[inline(never)]
fn schedule(block_reason: Option<BlockReason>) {
    let (current_ctx_ptr, next_ctx_ptr) = {
        let mut guard = SCHEDULER.lock();
        let s = guard.as_mut().expect("scheduler not initialised");

        s.reap();

        let current_ctx_ptr: *mut KernelContext = match s.current.take() {
            Some(mut cur) => {
                match block_reason {
                    Some(reason) => cur.block(reason),
                    None => cur.state = TaskState::Ready,
                }
                s.run_queue.push_back(cur);
                &mut s.run_queue.back_mut().unwrap().context as *mut KernelContext
            }
            None => &mut s.idle.context as *mut KernelContext,
        };

        let next_ctx_ptr: *const KernelContext = match s.pick_next() {
            Some(mut next) => {
                next.state = TaskState::Running;
                let ptr = &next.context as *const KernelContext;
                s.current = Some(next);
                ptr
            }
            None => {
                s.idle.state = TaskState::Running;
                &s.idle.context as *const KernelContext
            }
        };

        (current_ctx_ptr, next_ctx_ptr)
    };

    unsafe { KernelContext::switch_to(current_ctx_ptr, next_ctx_ptr) };
}

/// Lock the scheduler and run `f` with a mutable reference to it.
///
/// Panics if the scheduler has not been initialised.
fn with_scheduler<R>(f: impl FnOnce(&mut Scheduler) -> R) -> R {
    let mut guard = SCHEDULER.lock();
    let s = guard.as_mut().expect("scheduler not initialised");
    f(s)
}

/// The idle task body.
///
/// Runs when no other task is ready. Spins on `hlt` so the CPU enters a
/// low-power state between interrupts.
fn idle_task() -> ! {
    loop {
        Platform::halt();
    }
}

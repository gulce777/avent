//! Architecture-agnostic task context interface.
//!
//! A [`TaskContext`] captures the minimum CPU state needed to suspend a task
//! and resume it later at exactly the point it was interrupted. The concrete
//! register layout and the `switch_to` assembly stub live in the architecture-specific
//! backend (`arch/<target>/context.rs`).
//!
//! Only **callee-saved** registers need to be stored. Caller-saved registers are, by definition,
//! already saved by the compiler at every call site, including the call to `switch_to`.

use crate::arch::Platform;

/// The interface every architecture backend must implement for task contexts.
///
/// Implementors store the callee-saved register state for one task and provide the
/// `switch_to` primitive that performs the actual CPU context switch.
pub trait TaskContext: Sized + Send + 'static {
    /// Initialise a context for a new task.
    ///
    /// # Safety
    ///
    /// - `stack_top` must point to valid, exclusively owned, writable memory
    ///   that remains live for the entire lifetime of the task.
    /// - `stack_top` must be 16-byte aligned.
    unsafe fn new(entry: fn() -> !, stack_top: *mut u8) -> Self;

    /// Suspend the current task and resume another.
    ///
    /// Saves the callee-saved registers of the calling task into `*current`,
    /// then loads the callee-saved registers from `*next` and returns into
    /// the `next` task's execution context.
    ///
    /// # Safety
    ///
    /// - `current` must point to the [`TaskContext`] of the task that is
    ///   being suspended. It must be valid and exclusively accessible.
    /// - `next` must point to the [`TaskContext`] of the task to resume.
    ///   It must have been initialised by [`new`](Self::new) or by a prior
    ///   `switch_to` call that saved into it.
    /// - No other thread or interrupt handler may access `*current` or
    ///   `*next` for the duration of the switch.
    unsafe fn switch_to(current: *mut Self, next: *const Self);
}

/// The task context type for the current compilation target.
///
/// Resolves to `Platform::Context`, hides the generic parameter from all
/// call sites. Use this everywhere instead of the concrete backend type.
pub type KernelContext = <Platform as crate::arch::Arch>::Context;

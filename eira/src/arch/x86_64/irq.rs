//! IRQ routing and registry.

use core::sync::atomic::{AtomicPtr, Ordering};

pub type IrqHandler = fn();

pub const IRQ_BASE: u8 = 32;
pub const IRQ_MAX: usize = 224;

/// Lock-free array of interrupt handler function pointers.
///
/// We use `AtomicPtr` to allow drivers to register handlers dynamically
/// without requiring a lock (which is dangerous in interrupt contexts).
static IRQ_HANDLERS: [AtomicPtr<()>; IRQ_MAX] = {
    const EMPTY: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
    [EMPTY; IRQ_MAX]
};

/// Register a handler function for a specific IRQ line (0-based).
///
/// For example, `register_irq(0, timer_tick)` will bind the timer.
pub fn register_irq(irq: u8, handler: IrqHandler) {
    assert!((irq as usize) < IRQ_MAX, "IRQ out of bounds");

    let ptr = handler as *mut ();
    IRQ_HANDLERS[irq as usize].store(ptr, Ordering::Release);
    log::debug!("Registered IRQ handler for line {}", irq);
}

/// Dispatch an incoming IRQ to its registered handler, if any.
///
/// Called by the architecture-specific trap handler.
#[inline]
pub fn dispatch(irq: u8) {
    if (irq as usize) >= IRQ_MAX {
        return;
    }

    let ptr = IRQ_HANDLERS[irq as usize].load(Ordering::Acquire);
    if !ptr.is_null() {
        // SAFETY: The pointer was created from a valid `IrqHandler` (fn())
        // during registration.
        let handler: IrqHandler = unsafe { core::mem::transmute(ptr) };
        handler();
    } else {
        log::warn!("unhandled irq received: {}", irq);
    }
}

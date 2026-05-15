//! Serial output driver.
//!
//! Provides a platform-independent [`print!`] and [`println!`] interface
//! backed by the architecture-specific UART implementation.
//!
//! # Usage
//!
//! Call [`init`] once during early boot, then use [`print!`]/[`println!`] anywhere in the kernel.

use core::fmt;
use core::fmt::Write;

#[cfg(target_arch = "x86_64")]
mod uart16550;

#[cfg(target_arch = "aarch64")]
mod pl011;

#[cfg(target_arch = "aarch64")]
use pl011::Pl011 as UartImpl;
#[cfg(target_arch = "x86_64")]
use uart16550::Uart16550 as UartImpl;

static SERIAL: spin::Mutex<Option<UartImpl>> = spin::Mutex::new(None);

/// Initialize the serial port.
///
/// Must be called exactly once before any [`print!`]/[`println!`] invocation.
pub fn init(hhdm_offset: usize) {
    // SAFETY: We initialise the UART at the well-known platform address.
    // This is called once in early boot before any concurrent access.

    #[cfg(target_arch = "x86_64")]
    {
        // Man, x86_64 is so nice. Serial ports use port I/O, which completely
        // ignores the MMU and all that paging bullshit. You just `out dx, al`.
        let _ = hhdm_offset;

        let uart = unsafe { UartImpl::new(0x2F8) };
        *SERIAL.lock() = Some(uart);
    }

    #[cfg(target_arch = "aarch64")]
    {
        // TODO(aarch64): i literally cannot even print "hi" until i write an
        // entire VMM from scratch just to map this address.

        let _ = hhdm_offset;
    }
}

/// Write a formatted string to the serial port.
///
/// Silently drops output if [`init`] has not been called yet.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments<'_>) {
    if let Some(uart) = SERIAL.lock().as_mut() {
        // Infallible: UART write never returns an error in our driver.
        let _ = uart.write_fmt(args);
    }
}

/// Print formatted text to the serial port, without a trailing newline.
///
/// Mirrors the standard [`std::print!`] macro.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*))
    };
}

/// Print formatted text to the serial port, with a trailing newline.
///
/// Mirrors the standard [`std::println!`] macro.
#[macro_export]
macro_rules! println {
    ()            => { $crate::print!("\n") };
    ($($arg:tt)*) => { $crate::print!("{}\n", format_args!($($arg)*)) };
}

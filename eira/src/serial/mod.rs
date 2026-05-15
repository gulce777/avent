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

#[cfg(target_arch = "x86_64")]
use uart16550::Uart16550 as UartImpl;

#[cfg(target_arch = "x86_64")]
static SERIAL: spin::Mutex<Option<UartImpl>> = spin::Mutex::new(None);

/// Initialize the serial port.
///
/// Must be called exactly once before any [`print!`]/[`println!`] invocation.
pub fn init() {
    // SAFETY: We initialize the UART at the well-known platform address.
    // This is called once in early boot before any concurrent access.

    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: 0x2F8 is the standard COM2 base port address.
        // Called once in early boot before any concurrent access.
        let uart = unsafe { UartImpl::new(0x2F8) };
        *SERIAL.lock() = Some(uart);
    }

    #[cfg(target_arch = "aarch64")]
    {
        // TODO: aarch64 serial output is not yet implemented.
    }
}

/// Write a formatted string to the serial port.
///
/// Silently drops output if [`init`] has not been called yet.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments<'_>) {
    #[cfg(target_arch = "x86_64")]
    if let Some(uart) = SERIAL.lock().as_mut() {
        // Infallible: UART write never returns an error in our driver.
        let _ = uart.write_fmt(args);
    }
}

/// Print formatted text to the serial port without a trailing newline.
///
/// Mirrors the standard [`std::print!`] macro.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*))
    };
}

/// Print formatted text to the serial port with a trailing newline.
///
/// Mirrors the standard [`std::println!`] macro.
#[macro_export]
macro_rules! println {
    ()            => { $crate::print!("\n") };
    ($($arg:tt)*) => { $crate::print!("{}\n", format_args!($($arg)*)) };
}

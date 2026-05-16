//! QEMU ISA debug-exit device.
//!
//! When QEMU is launched with
//!
//! ```text
//! -device isa-debug-exit,iobase=0xf4,iosize=0x04
//! ```
//!
//! a single 32-bit write to port `0x4` causes QEMU to exit with
//! code `(value << 1) | 1`.
//!
//! `xtask` checks for exit code 33 to determine CI pass/fail.
//!
//! # Usage
//!
//! This module is only compiled with the `kernel-tests` feature and is
//! called exclusively by [`runner::run_all`](crate::test::runner::run_all).

use crate::arch::Arch;

/// Value written to the debug-exit port to signal all tests passed.
pub const EXIT_SUCCESS: u32 = 0x10;

/// Value written to the debug-exit port to signal one or more tests failed.
pub const EXIT_FAILURE: u32 = 0x11;

/// Terminate QEMU (or the physical machine, if somehow reached) with the
/// given exit code.
///
/// On x86_64 this writes to the ISA debug-exit port.
/// On aarch64 this invokes PSCI `SYSTEM_OFF` (no status code supported).
///
/// # Safety
///
/// Performs raw I/O port writes (x86_64) or a HVC/SMC call (aarch64).
/// Must only be called once, at the very end of the test run.
pub fn exit(code: u32) -> ! {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Writing to 0xf4 is safe when QEMU is started with
    // `-device isa-debug-exit,iobase=0xf4,iosize=0x04`. On real hardware
    // this port is typically unused; worst case the write is a no-op.
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx")  0xf4_u16,
            in("eax") code,
            options(nomem, nostack),
        );
    }

    #[cfg(target_arch = "aarch64")]
    // PSCI 0x84000008 = SYSTEM_OFF. Works with `-machine virt` and
    // `-cpu cortex-a57`. Cannot encode a status code.
    unsafe {
        core::arch::asm!(
            "mov x0, 0x84000000",
            "orr x0, x0, 8",
            "hvc #0",
            options(nomem, nostack, preserves_flags),
        );
    }

    loop {
        crate::arch::Platform::halt();
    }
}

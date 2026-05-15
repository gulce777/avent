//! UART 16550 driver for x86_64.
//!
//! Drives the 16550 UART via x86 port-mapped I/O. Register accesses use
//! `in`/`out` instructions.

use core::fmt;

/// Line status register bit 5 - Transmit holding register empty.
/// When set, the UART is ready to accept a new byte for transmission.
const LSR_THRE: u8 = 1 << 5;

/// UART 16550 driver.
///
/// All register accesses go through port-mapped I/O (`in`/`out` instructions).
/// The struct is zero-sized — it only carries the base port as a const.
pub struct Uart16550 {
    base: u16,
}

impl Uart16550 {
    /// Initialise and return a new UART instance at `base`.
    ///
    /// Configures the hardware and leaves it ready to transmit.
    pub unsafe fn new(base: u16) -> Self {
        let uart = Self { base };
        unsafe { uart.init() };
        uart
    }

    /// Configure the UART hardware.
    ///
    /// Sets the baud rate to 115200 (divisor 1), format to 8N1, disables
    /// interrupts and resets the FIFOs. Called once by [`new`](Self::new).
    ///
    /// # Safety
    ///
    /// No other code may access the same UART port concurrently.
    unsafe fn init(&self) {
        unsafe {
            // Disable all interrupts.
            self.write_reg(1, 0x00);

            // Enable DLAB to set baud rate divisor.
            self.write_reg(3, 0x80);

            // Divisor to 1 (115200 baud)
            self.write_reg(0, 0x01); // LSB
            self.write_reg(1, 0x00); // MSB

            // 8 data bits, no parity, 1 stop bit (8N1). Clears DLAB.
            self.write_reg(3, 0x03);

            // Enable and reset FIFOs.
            self.write_reg(2, 0xC7);
            self.write_reg(4, 0x03);
        }
    }

    /// Block until the transmit holding register is empty, then send `byte`.
    fn write_byte(&self, byte: u8) {
        // Spin until the transmit holding register is empty.
        // SAFETY: reading the LSR is always safe.
        while unsafe { self.read_reg(5) } & LSR_THRE == 0 {
            core::hint::spin_loop();
        }

        // SAFETY: transmit register is empty.
        unsafe { self.write_reg(0, byte) };
    }

    /// Read from a UART register at `base + offset`.
    ///
    /// # Safety
    ///
    /// `offset` must be a valid UART register offset (0–7).
    #[inline]
    unsafe fn read_reg(&self, offset: u16) -> u8 {
        let mut val: u8;
        // SAFETY: caller guarantees offset is valid.
        unsafe {
            core::arch::asm!(
                "in al, dx",
                in("dx") self.base + offset,
                out("al") val,
                options(nomem, nostack, preserves_flags),
            );
        }
        val
    }

    /// Write to a UART register at `base + offset`.
    ///
    /// # Safety
    ///
    /// `offset` must be a valid UART register offset (0–7).
    #[inline]
    unsafe fn write_reg(&self, offset: u16, val: u8) {
        // SAFETY: caller guarantees offset is valid.
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") self.base + offset,
                in("al") val,
                options(nomem, nostack, preserves_flags),
            );
        }
    }
}

impl fmt::Write for Uart16550 {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}

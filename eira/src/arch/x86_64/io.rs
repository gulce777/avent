//! x86_64 port I/O operations.

use core::arch::asm;

#[inline]
pub unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val);
    }
}

#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", out("al") val, in("dx") port);
    }
    val
}

//! 8259 Programmable Interrupt Controller (PIC) setup.

use super::io::{inb, outb};

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const ICW1_INIT: u8 = 0x11;
const ICW4_8086: u8 = 0x01;

pub unsafe fn disable_lapic() {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "mov ecx, 0x1B",
            "rdmsr",
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack),
        );

        let lo = lo & !(1u32 << 11);
        core::arch::asm!(
            "mov ecx, 0x1B",
            "wrmsr",
            in("eax") lo,
            in("edx") hi,
            options(nomem, nostack),
        );
    }
    log::debug!("LAPIC disabled, PIC mode active");
}

/// Remap the PIC interrupts to vectors 32-47.
pub unsafe fn init() {
    log::info!("INITTING");

    unsafe {
        let _ = inb(PIC1_DATA);
        let _ = inb(PIC2_DATA);

        outb(PIC1_COMMAND, ICW1_INIT);
        outb(PIC2_COMMAND, ICW1_INIT);

        outb(PIC1_DATA, 32); // PIC1 starts at vector 32
        outb(PIC2_DATA, 40); // PIC2 starts at vector 40

        outb(PIC1_DATA, 4);
        outb(PIC2_DATA, 2);

        outb(PIC1_DATA, ICW4_8086);
        outb(PIC2_DATA, ICW4_8086);

        outb(PIC1_DATA, 0xFE);
        outb(PIC2_DATA, 0xFF);
    }
}

pub unsafe fn init_pit() {
    let divisor: u16 = 11931;

    unsafe {
        outb(0x43, 0x36);
        outb(0x40, (divisor & 0xFF) as u8);
        outb(0x40, (divisor >> 8) as u8);
    }
}

/// Send End of Interrupt (EOI) to the PIC.
pub unsafe fn send_eoi(irq: u8) {
    unsafe {
        if irq >= 8 {
            outb(PIC2_COMMAND, 0x20);
        }
        outb(PIC1_COMMAND, 0x20);
    }
}

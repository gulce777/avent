//! x86_64 interrupt descriptor table.
//!
//! # Layout
//!
//! 32 entries covering all CPU exceptions (vectors 0-31). Each entry is a
//! 16-byte gate descriptor. All gates are interrupt gates (IF cleared on
//! entry) with DPL 0.
//!
//! # Exception frame
//!
//! Every handler receives an [`ExceptionFrame`] on the stack, pushed by the
//! CPU (+ our stub for the error-code-less exceptions). The frame contains
//! all general-purpose registers, the CPU-pushed exception frame, CR2, CR3
//! and the vector number.

use core::cell::UnsafeCell;
use core::fmt;

use super::gdt::{IST_BP, IST_DF, IST_GP, IST_MCE, IST_NMI, IST_PF, IST_SS, KCODE_SELECTOR};
use super::irq::{self, IRQ_BASE};
use super::pic;
use crate::arch::Arch;

/// Interrupt gate, DPL 0, present.
///
/// Using interrupt gates means IF is cleared on entry.
const INTERRUPT_GATE: u8 = 0x8E;

/// A 16-byte IDT gate descriptor.
#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    flags: u8,
    offset_mid: u16,
    offset_high: u32,
    _reserved: u32,
}

impl IdtEntry {
    /// A zeroed, non-present gate.
    pub const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            flags: 0,
            offset_mid: 0,
            offset_high: 0,
            _reserved: 0,
        }
    }

    /// Build a present interrupt gate pointing at `handler`.
    ///
    /// `ist` is 1-based (1–7). Pass 0 for no dedicated stack.
    pub fn new(handler: unsafe extern "C" fn(), ist: u8) -> Self {
        let addr = handler as u64;
        Self {
            offset_low: addr as u16,
            selector: KCODE_SELECTOR,
            ist: ist & 0x7,
            flags: INTERRUPT_GATE,
            offset_mid: (addr >> 16) as u16,
            offset_high: (addr >> 32) as u32,
            _reserved: 0,
        }
    }
}

/// The kernel IDT.
#[repr(C, align(16))]
pub struct Idt {
    entries: UnsafeCell<[IdtEntry; 256]>,
}

// SAFETY: Only ever accessed from the owning CPU after init.
unsafe impl Sync for Idt {}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

impl Idt {
    /// Build and populate the IDT with all 32 exception handlers.
    pub const fn new() -> Self {
        Self {
            entries: UnsafeCell::new([IdtEntry::missing(); 256]),
        }
    }

    /// Load this IDT via `lidt`.
    ///
    /// # Safety
    ///
    /// - `self` must remain at a stable address for as long as it is loaded.
    /// - Must be called from ring-0 after GDT/TSS are already loaded.
    pub unsafe fn load(&self) {
        self.populate();

        let ptr = IdtPointer {
            limit: (core::mem::size_of::<Idt>() - 1) as u16,
            base: self as *const Idt as u64,
        };

        unsafe {
            core::arch::asm!(
                "lidt [{ptr}]",
                ptr = in(reg) &ptr,
                options(nostack, preserves_flags),
            );
        }

        log::debug!("IDT loaded (base={:#p})", self);
    }

    fn set(&self, vector: usize, handler: unsafe extern "C" fn(), ist: u8) {
        let entries = unsafe { &mut *self.entries.get() };
        entries[vector] = IdtEntry::new(handler, ist);
    }

    fn populate(&self) {
        self.set(0, isr_stub_0, 0);
        self.set(1, isr_stub_1, 0);
        self.set(2, isr_stub_2, IST_NMI);
        self.set(3, isr_stub_3, IST_BP);
        self.set(4, isr_stub_4, 0);
        self.set(5, isr_stub_5, 0);
        self.set(6, isr_stub_6, 0);
        self.set(7, isr_stub_7, 0);
        self.set(8, isr_stub_8, IST_DF);
        self.set(9, isr_stub_9, 0);
        self.set(10, isr_stub_10, 0);
        self.set(11, isr_stub_11, 0);
        self.set(12, isr_stub_12, IST_SS);
        self.set(13, isr_stub_13, IST_GP);
        self.set(14, isr_stub_14, IST_PF);
        self.set(15, isr_stub_15, 0);
        self.set(16, isr_stub_16, 0);
        self.set(17, isr_stub_17, 0);
        self.set(18, isr_stub_18, IST_MCE);
        self.set(19, isr_stub_19, 0);
        self.set(20, isr_stub_20, 0);
        self.set(21, isr_stub_21, 0);
        self.set(22, isr_stub_22, 0);
        self.set(23, isr_stub_23, 0);
        self.set(24, isr_stub_24, 0);
        self.set(25, isr_stub_25, 0);
        self.set(26, isr_stub_26, 0);
        self.set(27, isr_stub_27, 0);
        self.set(28, isr_stub_28, 0);
        self.set(29, isr_stub_29, 0);
        self.set(30, isr_stub_30, 0);
        self.set(31, isr_stub_31, 0);
        self.set(32, isr_stub_32, 0);
        self.set(33, isr_stub_33, 0);
    }
}

/// Full CPU state captured on exception entry.
#[derive(Debug)]
#[repr(C)]
pub struct ExceptionFrame {
    pub cr2: u64,
    pub cr3: u64,
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub rbp: u64,
    /// Exception vector (0-31).
    pub vector: u64,
    /// Error code pushed by CPU or 0 for exceptions without one.
    pub error_code: u64,
    // Pushed by CPU.
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl fmt::Display for ExceptionFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n\x1B[1;31meira fault.\x1B[0m\n")?;

        writeln!(
            f,
            "\x1B[90mreason     |\x1B[0m\x1B[1m #{} - {}\x1B[0m",
            self.vector,
            exception_name(self.vector as u8),
        )?;
        writeln!(
            f,
            "\x1B[90mlocation   |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.rip,
        )?;

        if self.vector == 14 {
            writeln!(
                f,
                "\x1B[90maddress    |\x1B[0m\x1B[1m {:#018x}\x1B[0m  \
                         \x1B[90m({})\x1B[0m",
                self.cr2,
                pf_description(self.error_code),
            )?;
        }

        writeln!(f)?;

        writeln!(
            f,
            "\x1B[90mrip        |\x1B[0m\x1B[1m {:#018x}\x1B[0m  \
                     \x1B[90mrsp    |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.rip, self.rsp,
        )?;
        writeln!(
            f,
            "\x1B[90mrflags     |\x1B[0m\x1B[1m {:#018x}\x1B[0m  \
                     \x1B[90mcs     |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.rflags, self.cs,
        )?;
        writeln!(
            f,
            "\x1B[90mss         |\x1B[0m\x1B[1m {:#018x}\x1B[0m  \
                     \x1B[90merror  |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.ss, self.error_code,
        )?;

        writeln!(f)?;

        writeln!(
            f,
            "\x1B[90mrax        |\x1B[0m\x1B[1m {:#018x}\x1B[0m  \
                     \x1B[90mrbx    |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.rax, self.rbx,
        )?;
        writeln!(
            f,
            "\x1B[90mrcx        |\x1B[0m\x1B[1m {:#018x}\x1B[0m  \
                     \x1B[90mrdx    |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.rcx, self.rdx,
        )?;
        writeln!(
            f,
            "\x1B[90mrsi        |\x1B[0m\x1B[1m {:#018x}\x1B[0m  \
                     \x1B[90mrdi    |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.rsi, self.rdi,
        )?;
        writeln!(
            f,
            "\x1B[90mrbp        |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.rbp,
        )?;
        writeln!(
            f,
            "\x1B[90mr8         |\x1B[0m\x1B[1m {:#018x}\x1B[0m  \
                     \x1B[90mr9     |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.r8, self.r9,
        )?;
        writeln!(
            f,
            "\x1B[90mr10        |\x1B[0m\x1B[1m {:#018x}\x1B[0m  \
                     \x1B[90mr11    |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.r10, self.r11,
        )?;
        writeln!(
            f,
            "\x1B[90mr12        |\x1B[0m\x1B[1m {:#018x}\x1B[0m  \
                     \x1B[90mr13    |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.r12, self.r13,
        )?;
        writeln!(
            f,
            "\x1B[90mr14        |\x1B[0m\x1B[1m {:#018x}\x1B[0m  \
                     \x1B[90mr15    |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.r14, self.r15,
        )?;

        writeln!(f)?;

        writeln!(
            f,
            "\x1B[90mcr2        |\x1B[0m\x1B[1m {:#018x}\x1B[0m  \
                     \x1B[90mcr3    |\x1B[0m\x1B[1m {:#018x}\x1B[0m",
            self.cr2, self.cr3,
        )?;

        writeln!(f)?;

        writeln!(f, "\x1B[90mstatus     |\x1B[0m\x1B[1m core halted\x1B[0m\n")?;
        writeln!(f, "\x1B[90mplease reset the machine.\x1B[0m")?;

        Ok(())
    }
}

/// Decode a `#PF` error code into a human-readable access description.
///
/// Bits: [0] P      0 = non-present page, 1 = protection violation
///       [1] W/R    0 = read,             1 = write
///       [2] U/S    0 = kernel-mode,      1 = user-mode
///       [4] I/D    0 = data access,      1 = instruction fetch
fn pf_description(ec: u64) -> &'static str {
    let present = ec & (1 << 0) != 0;
    let write = ec & (1 << 1) != 0;
    let user = ec & (1 << 2) != 0;
    let ifetch = ec & (1 << 4) != 0;

    match (present, write, user, ifetch) {
        (false, false, false, false) => "non-present read kernel-mode",
        (false, true, false, false) => "non-present write kernel-mode",
        (false, false, true, false) => "non-present read user-mode",
        (false, true, true, false) => "non-present write user-mode",
        (true, false, false, false) => "protection read kernel-mode",
        (true, true, false, false) => "protection write kernel-mode",
        (true, false, true, false) => "protection read user-mode",
        (true, true, true, false) => "protection write user-mode",
        (_, _, false, true) => "instruction fetch kernel-mode",
        (_, _, true, true) => "instruction fetch user-mode",
    }
}

/// Returns the mnemonic and description for a CPU exception vector.
fn exception_name(vector: u8) -> &'static str {
    match vector {
        0 => "#DE - Divide Error",
        1 => "#DB - Debug",
        2 => "NMI - Non-Maskable Interrupt",
        3 => "#BP - Breakpoint",
        4 => "#OF - Overflow",
        5 => "#BR - Bound Range Exceeded",
        6 => "#UD - Invalid Opcode",
        7 => "#NM - Device Not Available",
        8 => "#DF - Double Fault",
        9 => "Coprocessor Segment Overrun",
        10 => "#TS - Invalid TSS",
        11 => "#NP - Segment Not Present",
        12 => "#SS - Stack Segment Fault",
        13 => "#GP - General Protection Fault",
        14 => "#PF - Page Fault",
        15 => "Reserved",
        16 => "#MF - x87 Floating-Point Exception",
        17 => "#AC - Alignment Check",
        18 => "#MC - Machine Check",
        19 => "#XM - SIMD Floating-Point Exception",
        20 => "#VE - Virtualization Exception",
        21 => "#CP - Control Protection Exception",
        28 => "#HV - Hypervisor Injection",
        29 => "#VC - VMM Communication",
        30 => "#SX - Security Exception",
        _ => "Reserved",
    }
}

/// Common Rust-level exception dispatcher.
///
/// Called by every ISR stub after saving the full register state.
///
/// # Safety
///
/// `frame` must point to a valid [`ExceptionFrame`] on the exception stack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trap_handler(frame: &ExceptionFrame) {
    let vector = frame.vector as u8;

    if vector < IRQ_BASE {
        handle_cpu_exception(frame);
    } else {
        let irq_line = vector - IRQ_BASE;

        irq::dispatch(irq_line);

        unsafe { pic::send_eoi(irq_line) };
    }
}

#[cold]
fn handle_cpu_exception(frame: &ExceptionFrame) -> ! {
    crate::arch::Platform::disable_interrupts();
    crate::print!("{}", frame);
    loop {
        crate::arch::Platform::halt();
    }
}

core::arch::global_asm!(
    //   [rsp+0]  vector
    //   [rsp+8]  error_code   (real or dummy 0)
    //   [rsp+16] rip
    //   [rsp+24] cs
    //   [rsp+32] rflags
    //   [rsp+40] rsp
    //   [rsp+48] ss
    "isr_common:",
    "push rbp",
    "push rax",
    "push rbx",
    "push rcx",
    "push rdx",
    "push rsi",
    "push rdi",
    "push r8",
    "push r9",
    "push r10",
    "push r11",
    "push r12",
    "push r13",
    "push r14",
    "push r15",
    "mov rax, cr3",
    "push rax",
    "mov rax, cr2",
    "push rax",
    "mov rdi, rsp",
    "mov rbx, rsp",
    "and rsp, -16",
    "call trap_handler",
    "mov rsp, rbx",
    "pop rax",
    "pop rax",
    "pop r15",
    "pop r14",
    "pop r13",
    "pop r12",
    "pop r11",
    "pop r10",
    "pop r9",
    "pop r8",
    "pop rdi",
    "pop rsi",
    "pop rdx",
    "pop rcx",
    "pop rbx",
    "pop rax",
    "pop rbp",
    "add rsp, 16",
    "iretq",
);

// Vectors WITH a CPU-pushed error code: 8, 10, 11, 12, 13, 14, 17, 21, 29, 30.
// All others need a dummy push.
macro_rules! isr_no_err {
    ($name:ident, $vec:expr) => {
        core::arch::global_asm!(
            concat!(stringify!($name), ":"),
            "push 0",               // dummy error code
            concat!("push ", $vec), // vector
            "jmp isr_common",
        );
    };
}

macro_rules! isr_with_err {
    ($name:ident, $vec:expr) => {
        core::arch::global_asm!(
            concat!(stringify!($name), ":"),
            // error code already on stack
            concat!("push ", $vec), // vector
            "jmp isr_common",
        );
    };
}

isr_no_err!(isr_stub_0, 0);
isr_no_err!(isr_stub_1, 1);
isr_no_err!(isr_stub_2, 2);
isr_no_err!(isr_stub_3, 3);
isr_no_err!(isr_stub_4, 4);
isr_no_err!(isr_stub_5, 5);
isr_no_err!(isr_stub_6, 6);
isr_no_err!(isr_stub_7, 7);
isr_with_err!(isr_stub_8, 8);
isr_no_err!(isr_stub_9, 9);
isr_with_err!(isr_stub_10, 10);
isr_with_err!(isr_stub_11, 11);
isr_with_err!(isr_stub_12, 12);
isr_with_err!(isr_stub_13, 13);
isr_with_err!(isr_stub_14, 14);
isr_no_err!(isr_stub_15, 15);
isr_no_err!(isr_stub_16, 16);
isr_with_err!(isr_stub_17, 17);
isr_no_err!(isr_stub_18, 18);
isr_no_err!(isr_stub_19, 19);
isr_no_err!(isr_stub_20, 20);
isr_with_err!(isr_stub_21, 21);
isr_no_err!(isr_stub_22, 22);
isr_no_err!(isr_stub_23, 23);
isr_no_err!(isr_stub_24, 24);
isr_no_err!(isr_stub_25, 25);
isr_no_err!(isr_stub_26, 26);
isr_no_err!(isr_stub_27, 27);
isr_no_err!(isr_stub_28, 28);
isr_with_err!(isr_stub_29, 29);
isr_with_err!(isr_stub_30, 30);
isr_no_err!(isr_stub_31, 31);

isr_no_err!(isr_stub_32, 32);
isr_no_err!(isr_stub_33, 33);

// Declare all stub symbols so Rust can take their addresses.
unsafe extern "C" {
    fn isr_stub_0();
    fn isr_stub_1();
    fn isr_stub_2();
    fn isr_stub_3();
    fn isr_stub_4();
    fn isr_stub_5();
    fn isr_stub_6();
    fn isr_stub_7();
    fn isr_stub_8();
    fn isr_stub_9();
    fn isr_stub_10();
    fn isr_stub_11();
    fn isr_stub_12();
    fn isr_stub_13();
    fn isr_stub_14();
    fn isr_stub_15();
    fn isr_stub_16();
    fn isr_stub_17();
    fn isr_stub_18();
    fn isr_stub_19();
    fn isr_stub_20();
    fn isr_stub_21();
    fn isr_stub_22();
    fn isr_stub_23();
    fn isr_stub_24();
    fn isr_stub_25();
    fn isr_stub_26();
    fn isr_stub_27();
    fn isr_stub_28();
    fn isr_stub_29();
    fn isr_stub_30();
    fn isr_stub_31();
    fn isr_stub_32();
    fn isr_stub_33();
}

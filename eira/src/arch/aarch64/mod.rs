//! AArch64 platform implementation.

mod entry {
    core::arch::global_asm!(include_str!("entry.s"));
}

pub mod imp;

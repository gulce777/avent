//! Architecture-specific implementations.
//!
//! Exposes a unified interface over the supported targets.
//!
//! # Supported architectures
//!
//! - `x86_64`
//! - `aarch64`

#[cfg(target_arch = "x86_64")]
mod x86_64;
#[cfg(target_arch = "x86_64")]
pub use x86_64::{disable_interrupts, enable_sse, halt};

#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(target_arch = "aarch64")]
pub use aarch64::{disable_interrupts, halt};

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("unsupported architecture, supported targets: x86_64, aarch64");

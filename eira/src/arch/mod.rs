//! Architecture-specific implementations.
//!
//! Exposes a unified [`Arch`] trait implemented by every supported target.
//! Call sites use the [`Cpu`] type alias and never depend on a concrete
//! backend directly.
//!
//! # Supported architectures
//!
//! - `x86_64`
//! - `aarch64`

#[cfg(target_arch = "x86_64")]
mod x86_64;
#[cfg(target_arch = "x86_64")]
pub use x86_64::enable_sse;
#[cfg(target_arch = "x86_64")]
use x86_64::imp::X86_64 as PlatformImpl;

#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(target_arch = "aarch64")]
use aarch64::imp::AArch64 as PlatformImpl;

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("unsupported architecture, supported targets: x86_64, aarch64");

/// The current platform's architecture backend.
///
/// A compile-time alias for the concrete [`Arch`] implementor selected for
/// the target architecture. Always prefer `Platform` over the underlying type.
pub type Platform = PlatformImpl;

/// The interface every architecture backend must implement.
///
/// All methods are static. The implementing type is always zero-sized struct
/// with no runtime representation.
pub trait Arch {
    /// Halt the current CPU core until the next interrupt or event.
    ///
    /// Always call this inside a loop, the core will resume execution
    /// after any interrupt fires.
    fn halt();

    /// Disable all hardware interrupts on the current core.
    ///
    /// After this returns, no external interrupts will be delivered until
    /// they are explicitly re-enabled.
    fn disable_interrupts();
}

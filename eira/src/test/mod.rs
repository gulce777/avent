//! Kernel test framework.
//!
//! # Overview
//!
//! A test framework that runs entirely inside the kernel.
//!
//! # How it works
//!
//! 1. Every test is registered at compile time by placing a `&TestCase`
//!    pointer into the `.test_cases` linker section (via [`kernel_test!`]).
//! 2. At boot, [`runner::run_all`] iterates that section, calls each test,
//!    and prints a result line to the serial port.
//! 3. When all tests finish, the kernel signals QEMU via the ISA debug-exit
//!    device so CI can read the pass/fail exit code.
//!
//! # Feature gate
//!
//! Everything in this module (and every `tests.rs` file) is compiled ONLY when the `kernel-tests`
//! Cargo feature is enabled.
//!
//! # Writing a test
//!
//! ```rust
//! use crate::{kernel_test, kassert_eq, test::TestKind};
//!
//! kernel_test!(frame_is_page_aligned, TestKind::Physical, {
//!   let frame = crate::mm::allocate();
//!   kassert_eq!(frame.base().as_usize() % 4096, 0)?;
//!   crate::mm::deallocate(frame);
//! });
//! ```

pub mod macros;
pub mod qemu;
pub mod runner;

/// A single registered test case.
///
/// Instances of this type are created by [`kernel_test!`] macro and stored in the
/// `.test_cases` linker section. You never construct one by hand.
pub struct TestCase {
    /// Short identifier, e.g. `"frame_is_page_aligned"`
    pub name: &'static str,

    /// Rust module path at the call site, e.g. `"eira::mm:tests"`
    pub module: &'static str,

    /// Source file
    pub file: &'static str,

    /// Source line
    pub line: u32,

    /// Category used by the runner to group and filter tests
    pub kind: TestKind,

    /// The test body. Returns [`TestResult`] and never panics (ideally)
    pub run: fn() -> TestResult,
}

// SAFETY: TestCase only contains 'static data and a fn pointer.
// No interior mutability, safe to share across (theoretical) cores.
unsafe impl Sync for TestCase {}

impl core::fmt::Debug for TestCase {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "TestCase({}::{})", self.module, self.name)
    }
}

/// Broad category for a test.
///
/// The runner uses this to print section headers and, in the future, to
/// apply filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestKind {
    /// Pure logic, no side effects on kernel state.
    ///
    /// Address arithmetic, bitmap bit manipulation, etc.
    Unit,

    /// Exercies the physical memory allocator.
    ///
    /// These tests allocate and deallocate real frames.
    Physical,

    /// Requires CPU-specific state: GDT, IDT, TSS, MSRs, etc.
    ///
    /// Only valid after [`arch::Platform::init_cpu`] returns.
    Arch,

    /// Anything that doesn't fit the above.
    ///
    /// Printed last and flagged so slow tests are obvious in CI.
    Integration,
}

impl TestKind {
    /// Short display label used in the section header.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Physical => "physical",
            Self::Arch => "arch",
            Self::Integration => "integration",
        }
    }
}

/// The outcome of running a single [`TestCase`].
#[derive(Debug)]
pub enum TestResult {
    /// Test completed without assertion failures.
    Pass,

    /// An assertion macro fired and returned this failure description.
    Fail(FailInfo),

    /// Test was intentionally skipped (e.g. wrong architecture).
    Skipped(&'static str),
}

impl TestResult {
    /// Returns `true` for [`TestResult::Pass`] and [`TestResult::Skipped`].
    #[inline]
    pub fn is_ok(&self) -> bool {
        !matches!(self, Self::Fail(_))
    }
}

/// Structured information about a test assertion failure.
///
/// Produced by [`kassert!`], [`kassert_eq!`] and [`kassert_ne!`].
/// Passed through `?` from the inner closure inside [`kernel_test!`].
#[derive(Debug)]
pub struct FailInfo {
    /// Human-readable description, e.g. `"assertion failed: left == right"`.
    pub message: &'static str,

    /// Source file where the assertion fired
    pub file: &'static str,

    /// Source line where the assertion fired
    pub line: u32,

    /// Left-hand side of a binary assertion, stored as `u64` for display.
    ///
    /// `None` for plain [`kassert!`].
    pub left: Option<u64>,

    /// Right-hand side of a binary assertion.
    pub right: Option<u64>,
}

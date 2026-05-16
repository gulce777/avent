//! Assertion and reqitration macros for the kernel test framework.
//!
//! # Assertion macros
//!
//! All assertion macros return `Err(FailInfo)` on failure. They are designed to be used
//! inside the closure that [`kernel_test!`] generates, where `?` propagates a `FailInfo` up to
//! the runner. **Do not use them outside a `kernel_test!` body.**

/// Assert that `$cond` is true.
///
/// Returns `Err(FailInfo)` on failure (use `?` to propagate).
///
/// # Example
///
/// ```rust
/// kernel_test!(some_condition_holds, TestKind::Unit, {
///     kassert!(1 + 1 == 2)?;
/// })
/// ```
#[macro_export]
macro_rules! kassert {
    ($cond:expr) => {
        if !$cond {
            return $crate::test::TestResult::Fail($crate::test::FailInfo {
                message: concat!("assertion failed: `", stringify!($cond), "`"),
                file: file!(),
                line: line!(),
                left: None,
                right: None,
            });
        }
    };
}

/// Assert that `$left == $right`.
///
/// On failure, the runner prints both sides as `u64` hex values so you can
/// see exactly what went wrong without a heap-allocated formatter.
///
/// Both operands are cast to `u64` for storage, this covers `usize`,
/// `u32`, `u16`, `u8`, and pointer-sized integers on 64-bit targets.
///
/// # Example
///
/// ```rust
/// kernel_test!(frame_aligned, TestKind::Physical, {
///     let frame = crate::mm::allocate();
///     kassert_eq!(frame.base().as_usize() % 4096, 0)?;
///     crate::mm::deallocate(frame);
/// });
/// ```
#[macro_export]
macro_rules! kassert_eq {
    ($left:expr, $right:expr) => {{
        let l = ($left) as u64;
        let r = ($right) as u64;
        if l != r {
            return $crate::test::TestResult::Fail($crate::test::FailInfo {
                message: concat!(
                    "assertion failed: `",
                    stringify!($left),
                    " == ",
                    stringify!($right),
                    "`",
                ),
                file: file!(),
                line: line!(),
                left: Some(l),
                right: Some(r),
            });
        }
    }};
}

/// Assert that `$left != $right`.
///
/// # Example
///
/// ```rust
/// kernel_test!(two_frames_differ, TestKind::Physical, {
///     let a = crate::mm::allocate();
///     let b = crate::mm::allocate();
///     kassert_ne!(a.base().as_usize(), b.base().as_usize())?;
///     crate::mm::deallocate(a);
///     crate::mm::deallocate(b);
/// });
/// ```
#[macro_export]
macro_rules! kassert_ne {
    ($left:expr, $right:expr) => {{
        let l = ($left) as u64;
        let r = ($right) as u64;
        if l == r {
            return $crate::test::TestResult::Fail($crate::test::FailInfo {
                message: concat!(
                    "assertion failed: `",
                    stringify!($left),
                    " != ",
                    stringify!($right),
                    "`",
                ),
                file: file!(),
                line: line!(),
                left: Some(l),
                right: Some(r),
            });
        }
    }};
}

#[macro_export]
macro_rules! kskip {
    ($reason:expr) => {
        return $crate::test::TestResult::Skipped($reason);
    };
}

/// Register a kernel test case.
///
/// Emits a `#[link_section = ".test_cases"` static so the runner can
/// discover the test at boot.
///
/// # Syntax
///
/// ```rust
/// kernel_test!($name: ident, $kind: expr, { $body });
/// ```
///
/// - `$name`: unique identifier within the current module. Used as the
///   static name and the test's display name.
/// - `$kind`: a [`TestKind`](crate::test::TestKind) variant.
/// - `$body`: the test body. May use `kassert*!` macros with `?`.
///   Return type is inferred as `Result<(), FailInfo>`.
///
/// # How failure works
///
/// The body is wrapped in a `|| -> Result<(), FailInfo>` closure. Using `?`
/// inside the body short-circuits execution and returns the failure to the
/// runner.
///
/// # Example
///
/// ```rust
/// kernel_test!(bitmap_bit_roundtrip, TestKind::Unit, {
///     let val: u8 = 0b0000_0000;
///     let set = val | (1 << 3);
///     kassert_eq!(set, 0b0000_1000)?;
/// });
/// ```
///
/// # Linker section
///
/// Each invocation emits:
///
/// ```rust,ignore
/// #[used]
/// #[link_section = ".test_cases"]
/// static $name_TEST_CASE: &TestCase = &TestCase { ... };
/// ```
/// The static name gets a `_KTEST_` prefix to avoid collisions with normal
/// items in the module.
#[macro_export]
macro_rules! kernel_test {
    ($name:ident, $kind:expr, $body:block) => {
        #[allow(non_upper_case_globals)]
        #[used]
        #[unsafe(link_section = ".test_cases")]
        static $name: &$crate::test::TestCase = &$crate::test::TestCase {
            name: stringify!($name),
            module: module_path!(),
            file: file!(),
            line: line!(),
            kind: $kind,
            run: || -> $crate::test::TestResult {
                $body
                $crate::test::TestResult::Pass
            },
        };
    };
}

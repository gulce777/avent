//! Test runner discovers, executes and reports all registered kernel tests.
//!
//! Execution order
//!
//! Tests run in link order within each [`TestKind`] group:
//! `Unit` -> `Physical` -> `Arch` -> `Integration`

use super::{TestCase, TestKind, TestResult};

const RESET: &str = "\x1B[0m";
const DIM: &str = "\x1B[2m";
const DIM_BOLD: &str = "\x1B[1;2m";
const BOLD: &str = "\x1B[1m";
const GREEN: &str = "\x1B[1;32m";
const RED: &str = "\x1B[1;31m";
const YELLOW: &str = "\x1B[1;33m";

unsafe extern "C" {
    /// Provided by the linker script; marks the start of `.test_cases`.
    static __test_cases_start: u8;
    /// Provided by the linker script; marks the end of `.test_cases`.
    static __test_cases_end: u8;
}

/// Returns a slice over every [`TestCase`] registered via [`kernel_test!`].
///
/// # Safety
///
/// Reads raw linker-generated symbols. Only safe to call after the kernel
/// image is fully mapped (i.e., after `mm::init`).
pub fn all_tests() -> &'static [&'static TestCase] {
    // SAFETY:
    // - `__test_cases_start` / `__test_cases_end` are placed by the linker
    //   and are guaranteed to be valid, aligned, `'static` pointers.
    // - Each element is a `&'static TestCase` placed by `kernel_test!` via
    //   `#[link_section = ".test_cases"]`.
    // - No mutation ever occurs to this region after boot.
    unsafe {
        let start = core::ptr::addr_of!(__test_cases_start) as *const &TestCase;
        let end = core::ptr::addr_of!(__test_cases_end) as *const &TestCase;

        // `offset_from` is defined only for pointers into the same allocation;
        // both come from the linker section so this is sound.
        let len = end.offset_from(start) as usize;

        core::slice::from_raw_parts(start, len)
    }
}

#[derive(Default)]
struct Counts {
    passed: usize,
    failed: usize,
    skipped: usize,
}

impl Counts {
    fn total(&self) -> usize {
        self.passed + self.failed + self.skipped
    }
}

/// Run every registered test, print results to serial, then exit QEMU.
///
/// Call this once at the end of `kmain` when the `kernel-tests` feature
/// is active. It never returns — it exits QEMU when done.
///
/// # Panics
///
/// Does not panic by design. Individual test failures are caught by the
/// `Result`-returning closure inside [`kernel_test!`] and reported as
/// [`TestResult::Fail`] rather than bringing down the kernel.
///
pub fn run_all() -> ! {
    let all = all_tests();

    crate::println!("\n{BOLD}eira test run. running {} tests.{RESET}", all.len());

    let mut counts = Counts::default();

    for kind in [
        TestKind::Unit,
        TestKind::Physical,
        TestKind::Arch,
        TestKind::Integration,
    ] {
        let group: &[&TestCase] = &all
            .iter()
            .copied()
            .filter(|t| t.kind == kind)
            .collect::<arrayvec::ArrayVec<_, 256>>(); // no alloc yet; fixed cap

        if group.is_empty() {
            continue;
        }

        crate::println!("\n{DIM_BOLD}{}{RESET}", kind.label());

        for test in group {
            run_one(test, &mut counts);
        }
    }

    print_footer(&counts);

    let exit_code = if counts.failed == 0 {
        super::qemu::EXIT_SUCCESS
    } else {
        super::qemu::EXIT_FAILURE
    };

    super::qemu::exit(exit_code);
}

fn run_one(test: &TestCase, counts: &mut Counts) {
    crate::print!(
        "    {DIM}....{RESET}  {dim}{module}{RESET} :: {name}",
        dim = DIM,
        module = test.module,
        name = test.name,
    );

    let result = (test.run)();

    match &result {
        TestResult::Pass => {
            counts.passed += 1;
            crate::println!("\r    {GREEN}pass{RESET}");
        }

        TestResult::Fail(info) => {
            counts.failed += 1;
            crate::println!("\r    {RED}fail{RESET}");
            crate::println!("{DIM}            {message}{RESET}", message = info.message,);
            if let (Some(l), Some(r)) = (info.left, info.right) {
                crate::println!("            {DIM}left :{RESET}  {:#018x}", l);
                crate::println!("            {DIM}right:{RESET}  {:#018x}", r);
            }
            crate::println!(
                "            {DIM}at:     {}:{}{RESET}",
                info.file,
                info.line,
            );
        }

        TestResult::Skipped(reason) => {
            counts.skipped += 1;
            crate::println!(
                "\r    {YELLOW}skip{RESET}  {dim}{module}{RESET} :: {name}  {DIM}({reason}){RESET}",
                dim = DIM,
                module = test.module,
                name = test.name,
            );
        }
    }
}

fn print_footer(c: &Counts) {
    let status_color = if c.failed > 0 { RED } else { GREEN };

    crate::print!("\n\n  {status_color}{BOLD}result  {DIM}|{RESET}");
    crate::print!(
        "  {green}{} passed{RESET}",
        c.passed,
        green = if c.passed > 0 { GREEN } else { DIM }
    );
    crate::print!(
        "  {red}{} failed{RESET}",
        c.failed,
        red = if c.failed > 0 { RED } else { DIM },
    );
    crate::println!(
        "  {yellow}{} skipped{RESET}\n",
        c.skipped,
        yellow = if c.skipped > 0 { YELLOW } else { DIM }
    );
}

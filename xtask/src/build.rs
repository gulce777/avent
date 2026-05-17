use crate::{log_build, log_ok, workspace_root, Arch};
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;

pub fn build_kernel(arch: &Arch, release: bool) -> Result<()> {
    build_kernel_inner(arch, release, false)
}

pub fn build_kernel_with_tests(arch: &Arch, release: bool) -> Result<()> {
    build_kernel_inner(arch, release, true)
}

pub fn build_kernel_inner(arch: &Arch, release: bool, tests: bool) -> Result<()> {
    let root = workspace_root();
    let target_json = root.join("targets").join(arch.target_json());
    let linker_script = root.join(arch.linker_script());

    if !target_json.exists() {
        bail!(
            "Target JSON not found: {}\n\
             Create targets/{} to add support for this architecture.",
            target_json.display(),
            arch.target_json()
        );
    }

    if !linker_script.exists() {
        bail!(
            "Linker script not found: {}\n\
                 Make sure the file exists for the '{}' architecture.",
            linker_script.display(),
            arch.as_str()
        );
    }

    let profile_label = if release { "release" } else { "debug" };

    log_build!(
        "Compiling arc_kernel   [arch: {}  profile: {}]",
        arch.as_str(),
        profile_label,
    );

    let mut cmd = Command::new("cargo");
    cmd.current_dir(&root)
        .args(["build", "--package", "eira"])
        .arg("--target")
        .arg(&target_json)
        .args([
            "-Zbuild-std=core,compiler_builtins,alloc",
            "-Zbuild-std-features=compiler-builtins-mem",
            "-Zjson-target-spec",
        ]);

    if tests {
        cmd.args(["--features", "kernel-tests"]);
    }

    if release {
        cmd.arg("--release");
    }

    let status = cmd.status().context("Failed to run cargo build")?;

    if !status.success() {
        bail!("Kernel compilation failed");
    }

    let bin = kernel_bin_path(arch, release);
    let rel = bin.strip_prefix(&root).unwrap_or(&bin);
    log_ok!("Kernel compiled:   {}", rel.display(),);
    Ok(())
}

pub fn kernel_bin_path(arch: &Arch, release: bool) -> PathBuf {
    let root = workspace_root();
    let target_name = arch.target_json().replace(".json", "");
    let profile = if release { "release" } else { "debug" };
    root.join("target")
        .join(target_name)
        .join(profile)
        .join("eira")
}

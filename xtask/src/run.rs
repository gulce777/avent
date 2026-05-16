use crate::{Arch, build_dir, log_ok, log_run};
use anyhow::{Context, Result, bail};
use std::process::Command;

const QEMU_TEST_SUCCESS: i32 = 33;
const QEMU_TEST_FAILURE: i32 = 35;

pub fn run_qemu(arch: &Arch, uefi: bool) -> Result<()> {
    let mut cmd = base_cmd(arch, uefi)?;

    let status = cmd.status().context("Failed to start QEMU")?;
    if !status.success() {
        bail!("QEMU exited with an error");
    }

    Ok(())
}

pub fn run_qemu_tests(arch: &Arch, uefi: bool) -> Result<()> {
    let mut cmd = base_cmd(arch, uefi)?;

    if matches!(arch, Arch::X86_64) {
        cmd.args(["-device", "isa-debug-exit,iobase=0xf4,iosize=0x04"]);
    }

    cmd.args(["-no-reboot", "-display", "none"]);

    let status = cmd.status().context("Failed to start QEMU")?;

    match (arch, status.code()) {
        (Arch::X86_64, Some(QEMU_TEST_SUCCESS)) => {
            log_ok!("all tests passed");
            Ok(())
        }
        (Arch::X86_64, Some(QEMU_TEST_FAILURE)) => {
            bail!("one or more tests failed");
        }
        (Arch::X86_64, Some(code)) => {
            bail!("QEMU exited unexpectedly (code {code})");
        }
        (Arch::X86_64, None) => {
            bail!("QEMU killed by signal");
        }
        (Arch::Aarch64, _) => {
            log_ok!("QEMU exited, check serial output for test results");
            Ok(())
        }
    }
}

fn base_cmd(arch: &Arch, uefi: bool) -> Result<Command> {
    let qemu = arch.qemu_bin();
    if Command::new(qemu).arg("--version").output().is_err() {
        bail!("{qemu} not found.");
    }

    let build = build_dir();
    let iso_path = build.join(arch.iso());
    if !iso_path.exists() {
        bail!(
            "ISO not found: {}\nRun `cargo xtask iso` first.",
            iso_path.display()
        );
    }

    let mode = if uefi { "UEFI" } else { "BIOS" };
    log_run!("Starting QEMU   [arch: {}  mode: {mode}]", arch.as_str());

    let mut cmd = Command::new(qemu);
    let code = build.join("edk2-ovmf").join(arch.ovmf_code());
    let vars = build.join("edk2-ovmf").join(arch.ovmf_vars());

    match arch {
        Arch::X86_64 => {
            cmd.args(["-M", "q35", "-m", "256M"]);
            cmd.args(["-cdrom", iso_path.to_str().unwrap()]);
            cmd.args(["-serial", "file:uefi_boot.log"]);
            cmd.args(["-serial", "stdio"]);

            if uefi {
                if !code.exists() {
                    bail!("OVMF firmware not found, run `cargo xtask fetch`");
                }
                cmd.args([
                    "-drive",
                    &format!("if=pflash,format=raw,readonly=on,file={}", code.display()),
                ]);
                if vars.exists() {
                    cmd.args([
                        "-drive",
                        &format!("if=pflash,format=raw,file={}", vars.display()),
                    ]);
                }
            } else {
                cmd.args(["-boot", "d"]);
            }
        }
        Arch::Aarch64 => {
            if !code.exists() {
                bail!("OVMF firmware not found, run `cargo xtask fetch`");
            }
            cmd.args(["-M", "virt", "-cpu", "cortex-a57", "-m", "256M"]);
            cmd.args(["-cdrom", iso_path.to_str().unwrap()]);
            cmd.args(["-serial", "stdio"]);
            cmd.args([
                "-device",
                "ramfb",
                "-device",
                "qemu-xhci",
                "-device",
                "usb-kbd",
                "-device",
                "usb-mouse",
            ]);
            cmd.args([
                "-drive",
                &format!("if=pflash,format=raw,readonly=on,file={}", code.display()),
            ]);
            if vars.exists() {
                cmd.args([
                    "-drive",
                    &format!("if=pflash,format=raw,file={}", vars.display()),
                ]);
            }
        }
    }

    Ok(cmd)
}

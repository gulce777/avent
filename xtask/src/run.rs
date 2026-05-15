use crate::{Arch, build_dir, log_run};
use anyhow::{Context, Result, bail};
use std::process::Command;

pub fn run_qemu(arch: &Arch, uefi: bool) -> Result<()> {
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

    let status = cmd.status().context("Failed to start QEMU")?;
    if !status.success() {
        bail!("QEMU exited with an error");
    }

    Ok(())
}

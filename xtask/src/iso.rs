use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::build::kernel_bin_path;
use crate::{Arch, build_dir, log_info, log_ok, log_step, log_warn, workspace_root};

pub fn create_iso(arch: &Arch, release: bool) -> Result<()> {
    log_info!("Building ISO image   [arch: {}]", arch.as_str());

    let root = workspace_root();
    let build = build_dir();
    let limine = build.join("limine");
    let iso_root = build.join("iso_root");

    if iso_root.exists() {
        fs::remove_dir_all(&iso_root)?;
    }

    let limine_dir = iso_root.join("boot").join("limine");
    let efi_dir = iso_root.join("EFI").join("BOOT");
    fs::create_dir_all(&limine_dir)?;
    fs::create_dir_all(&efi_dir)?;

    let kernel = kernel_bin_path(arch, release);
    if !kernel.exists() {
        bail!("Kernel binary not found:   {}", kernel.display());
    }
    fs::copy(&kernel, iso_root.join("boot").join("eira"))?;
    log_step!(1, 4, "Kernel binary copied");

    fs::copy(root.join("limine.conf"), limine_dir.join("limine.conf"))?;
    log_step!(2, 4, "limine.conf copied");

    let has_bios = matches!(arch, Arch::X86_64);

    if has_bios {
        for file in ["limine-bios.sys", "limine-bios-cd.bin"] {
            let src = limine.join(file);
            if src.exists() {
                fs::copy(&src, limine_dir.join(file))
                    .with_context(|| format!("Failed to copy {file}"))?;
            } else {
                bail!("{file} not found in limine. Run `cargo xtask fetch`");
            }
        }
    }

    {
        let file = "limine-uefi-cd.bin";
        let src = limine.join(file);
        if src.exists() {
            fs::copy(&src, limine_dir.join(file))
                .with_context(|| format!("Failed to copy {file}"))?;
        } else {
            bail!("{file} not found in limine. Run `cargo xtask fetch`");
        }
    }

    copy_efi_files(arch, &limine, &efi_dir)?;
    log_step!(3, 4, "Limine boot files staged");

    let iso_path = build.join(arch.iso());
    let mut xorriso_args: Vec<&str> = vec!["-as", "mkisofs"];

    if has_bios {
        xorriso_args.extend([
            "-b",
            "boot/limine/limine-bios-cd.bin",
            "-no-emul-boot",
            "-boot-load-size",
            "4",
            "-boot-info-table",
        ]);
    }

    xorriso_args.extend([
        "--efi-boot",
        "boot/limine/limine-uefi-cd.bin",
        "-efi-boot-part",
        "--efi-boot-image",
        "--protective-msdos-label",
    ]);

    let iso_path_str = iso_path.to_str().unwrap();
    let iso_root_str = iso_root.to_str().unwrap();
    xorriso_args.extend([iso_root_str, "-o", iso_path_str]);

    let status = Command::new("xorriso")
        .args(&xorriso_args)
        .status()
        .context("xorriso not found — install it with: sudo apt install xorriso")?;

    if !status.success() {
        bail!("xorriso failed to create the ISO");
    }

    log_step!(4, 4, "ISO finalised");
    log_ok!("ISO image ready:   {}", iso_path.display());

    Ok(())
}

fn copy_efi_files(arch: &Arch, limine_dir: &PathBuf, efi_dir: &PathBuf) -> Result<()> {
    let files: &[&str] = match arch {
        Arch::X86_64 => &["BOOTX64.EFI"],
        Arch::Aarch64 => &["BOOTAA64.EFI"],
    };

    for name in files {
        let src = limine_dir.join(name);
        if src.exists() {
            fs::copy(&src, efi_dir.join(name)).with_context(|| format!("Failed to copy {name}"))?;
        } else {
            log_warn!("{name} not found in limine");
        }
    }

    Ok(())
}

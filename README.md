<div align="center">

# eira

![Rust](https://img.shields.io/badge/rust-nightly-f4c2c2?style=flat-square&logo=rust&logoColor=8b4a4a&labelColor=fff0f3)
![License](https://img.shields.io/badge/license-MIT-c9b8e8?style=flat-square&labelColor=f0ebff)
![Architecture](https://img.shields.io/badge/arch-x86__64%20%7C%20aarch64-b8d8e8?style=flat-square&labelColor=ebf5ff)
![Last Commit](https://img.shields.io/github/last-commit/gulce777/eira?style=flat-square&labelColor=fff8eb&color=fde8b0)
![Repo Size](https://img.shields.io/github/repo-size/gulce777/eira?style=flat-square&labelColor=fce4ec&color=f8bbd0)

*a microkernel, built from scratch, in [rust](https://rust-lang.org/).*

</div>

---

## what is eira?

eira is a microkernel written in rust. targets both x86_64 and aarch64 systems.

## getting started

### prerequisites

- nightly rust
- `qemu-system-x86_64` and/or `qemu-system-aarch64`

### running

```sh
# run the kernel in qemu
cargo xtask run

# run with tests
cargo xtask test
```

### testing

eira has a custom in-kernel test framework. tests are registered at compile time via
a linker section (`.test_cases`) and discovered at boot.

```rust
kernel_test!(frame_is_page_aligned, TestKind::Physical, {
    let frame = mm::allocate();
    kassert_eq!(frame.base().as_usize() % 4096, 0);
    mm::deallocate(frame);
});
```

tests run grouped by kind (`Unit`, `Physical`, `Arch`, `Integration`), results are printed to
serial and qemu exits with a code ci can read.

## gallery

![panic](gallery/panic.png)

![testing](gallery/testing.png)

## license

[mit](LICENSE)

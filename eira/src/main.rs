#![no_std]
#![no_main]

mod arch;
mod logger;
mod mm;
mod serial;
#[cfg(feature = "kernel-tests")]
mod test;

use crate::arch::{Arch, Platform};
use crate::mm::PAGE_SIZE;
use crate::mm::address_space::{AddressSpace, KernelAddressSpace};
use crate::mm::paging::PageFlags;
use limine::request::{FramebufferRequest, HhdmRequest, MemmapRequest, StackSizeRequest};
use limine::{BaseRevision, RequestsEndMarker, RequestsStartMarker};

#[used]
#[unsafe(link_section = ".requests_start")]
static _REQUESTS_START: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static STACK_SIZE_REQUEST: StackSizeRequest = StackSizeRequest::new(0x10000);

#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemmapRequest = MemmapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".requests_end")]
static _REQUESTS_END: RequestsEndMarker = RequestsEndMarker::new();

/// Main kernel entry point.
///
/// # Safety
///
/// Must only be called ONCE, from the assembly stub, on the boot CPU.
#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    #[cfg(target_arch = "x86_64")]
    arch::enable_sse();

    serial::init();
    print!("\x1B[2J\x1B[H");

    logger::init();
    Platform::init_cpu();

    let memmap = MEMORY_MAP_REQUEST
        .response()
        .expect("no memory map response");
    let hhdm = HHDM_REQUEST.response().expect("no HHDM response");

    if BASE_REVISION.is_supported() {
        log::debug!("limine base revision supported");
    } else {
        panic!("incompatible bootloader: limine base revision is not supported");
    }

    unsafe { mm::init::init(memmap, hhdm) };

    #[cfg(feature = "kernel-tests")]
    crate::test::runner::run_all();

    log::info!("halting");
    loop {
        Platform::halt();
    }
}

/// Fill a rectangle on the framebuffer with a given ARGB colour.
///
/// # Safety
///
/// `fb_ptr` must point to a valid, mapped framebuffer region large enough to contain
/// all pixels in the rectangle defined by `(x, y, width, height)`.
unsafe fn draw_rect(
    fb_ptr: *mut u8,
    pitch: usize,
    bpp: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    argb: u32,
) {
    let [b, g, r, a] = argb.to_le_bytes();

    for row in y..(y + height) {
        for col in x..(x + width) {
            let offset = row * pitch + col * bpp;
            // SAFETY: caller guarantees the pointer and bounds are valid.
            unsafe {
                core::ptr::write_volatile(fb_ptr.add(offset), b);
                core::ptr::write_volatile(fb_ptr.add(offset + 1), g);
                core::ptr::write_volatile(fb_ptr.add(offset + 2), r);
                core::ptr::write_volatile(fb_ptr.add(offset + 3), a);
            }
        }
    }
}

#[cold]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    Platform::disable_interrupts();

    println!("\n\x1B[1;31meira fault.\x1B[0m\n");

    let reason = info.message();

    let (file, line) = if let Some(location) = info.location() {
        (location.file(), location.line())
    } else {
        ("unknown", 0)
    };

    println!("\x1B[90mreason     |\x1B[0m\x1B[1m {}\x1B[0m", reason);
    println!(
        "\x1B[90mlocation   |\x1B[0m\x1B[1m {}:{}\x1B[0m",
        file, line
    );
    println!("\x1B[90mstatus     |\x1B[0m\x1B[1m core halted\x1B[0m\n");

    println!("\x1B[90mplease reset the machine.\x1B[0m");

    loop {
        Platform::halt();
    }
}

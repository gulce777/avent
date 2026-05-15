#![no_std]
#![no_main]

mod arch;
mod serial;

use limine::request::{FramebufferRequest, HhdmRequest, StackSizeRequest};
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
    let hhdm_offset = HHDM_REQUEST
        .response()
        .expect("Limine did not provide HHDM offset")
        .offset as usize;

    serial::init(hhdm_offset);

    print!("\x1B[2J\x1B[H");
    println!("[arc] kernel starting");
    println!(
        "[arc] base revision supported: {}",
        BASE_REVISION.is_supported()
    );

    // If the bootloader set the revision field to 0, the requested revision is
    // supported. Any other value means an incompatible bootloader.
    assert!(
        BASE_REVISION.is_supported(),
        "Limine base version not supported"
    );

    if let Some(response) = FRAMEBUFFER_REQUEST.response() {
        let fbs = response.framebuffers();

        if let Some(fb) = fbs.first() {
            let pitch = fb.pitch as usize;
            let bpp = (fb.bpp / 8) as usize;

            // SAFETY: Limine guarantees `fb.address()` is a valid, mapped,
            // writable pointer to framebuffer memory for the lifetime of the
            // bootloader-reclaimable region.
            let fb_ptr = fb.address() as *mut u8;

            unsafe { draw_rect(fb_ptr, pitch, bpp, 100, 100, 200, 200, 0xFF_FF_FF_FF) };
        }
    }

    #[cfg(target_arch = "aarch64")]
    // SAFETY: `dsb sy` is a memory barrier with no side effects beyond ordering.
    unsafe {
        core::arch::asm!("dsb sy", options(nostack, nomem));
    }

    loop {
        arch::halt();
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

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        arch::halt();
    }
}

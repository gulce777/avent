#![no_std]
#![no_main]

mod arch;
mod logger;
mod mm;
mod serial;

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

    serial::init(0);
    logger::init();

    let memmap = MEMORY_MAP_REQUEST
        .response()
        .expect("no memory map response");
    let hhdm = HHDM_REQUEST.response().expect("no HHDM response");

    print!("\x1B[2J\x1B[H");

    unsafe { mm::init::init(memmap, hhdm) };

    run_allocator_tests();

    if BASE_REVISION.is_supported() {
        log::debug!("limine base revision supported");
    } else {
        panic!("incompatible bootloader: limine base revision is not supported");
    }

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

    log::info!("halting");
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

#[cold]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    arch::disable_interrupts();

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
        arch::halt();
    }
}

pub fn run_allocator_tests() {
    log::info!("starting physical memory tests...");

    let frame1 = mm::allocate();
    assert_eq!(
        frame1.base().as_usize() % 4096,
        0,
        "allocated frame is not 4K aligned!"
    );
    log::info!("test 1 passed: basic allocation and page alignment are correct.");

    let initial_free = mm::free_frames();
    let frame2 = mm::allocate();
    let after_alloc_free = mm::free_frames();

    assert_eq!(
        initial_free - 1,
        after_alloc_free,
        "free frame count did not decrease correctly after allocation!"
    );

    mm::deallocate(frame2);
    let after_dealloc_free = mm::free_frames();

    assert_eq!(
        initial_free, after_dealloc_free,
        "free frame count did not return to its original value after deallocation (memory leak)!"
    );
    log::info!("test 2 passed: frame tracking is correct.");

    let f_a = mm::allocate();
    let f_b = mm::allocate();
    let f_c = mm::allocate();

    let base_a = f_a.base();
    let base_b = f_b.base();
    let base_c = f_c.base();

    mm::deallocate(f_a);
    mm::deallocate(f_b);
    mm::deallocate(f_c);

    let f_c_again = mm::allocate();
    let f_b_again = mm::allocate();
    let f_a_again = mm::allocate();

    assert_eq!(
        f_c_again.base(),
        base_c,
        "lifo order is broken, expected f_c."
    );
    assert_eq!(
        f_b_again.base(),
        base_b,
        "lifo order is broken, expected f_b."
    );
    assert_eq!(
        f_a_again.base(),
        base_a,
        "lifo order is broken, expected f_a."
    );

    mm::deallocate(f_c_again);
    mm::deallocate(f_b_again);
    mm::deallocate(f_a_again);
    mm::deallocate(frame1);

    log::info!("test 3 passed: lifo chain is correct.");

    // calling `mm:deallocate` twice on the same OwnedFrame is a compile-time
    // error now. This panic is no longer expressible. THANKS RUST!
    /*
    let danger_frame = mm::allocate();
    mm::deallocate(danger_frame);
    log::warn!("a double-free panic should be triggered right now...");
    mm::deallocate(danger_frame);
    */

    log::info!("all physical memory allocation tests completed.");
}

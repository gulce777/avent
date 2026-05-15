#![no_std]
#![no_main]

use limine::request::{FramebufferRequest, StackSizeRequest};
use limine::{BaseRevision, RequestsEndMarker, RequestsStartMarker};

#[repr(C, align(8))]
struct LimineRequests {
    base: BaseRevision,
    fb: FramebufferRequest,
    stack: StackSizeRequest,
}

#[used]
#[unsafe(link_section = ".requests")]
static REQUESTS: LimineRequests = LimineRequests {
    base: BaseRevision::new(),
    fb: FramebufferRequest::new(),
    stack: StackSizeRequest::new(0x10000),
};

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
.section .text
.global _start_asm
_start_asm:
    adr x0, _exception_hang
    msr vbar_el1, x0

    mov x0, #(3 << 20)
    msr cpacr_el1, x0
    isb

    b _start

_exception_hang:
    b _exception_hang
"#
);

#[used]
#[unsafe(link_section = ".requests_start")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();
#[used]
#[unsafe(link_section = ".requests_end")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    if let Some(response) = REQUESTS.fb.response() {
        let fbs = response.framebuffers();

        if let Some(fb) = fbs.first() {
            let pitch = fb.pitch as usize;
            let bpp = (fb.bpp / 8) as usize;
            let fb_ptr = fb.address() as *mut u8;

            let size = 200;
            let start_x = 100;
            let start_y = 100;

            for y in start_y..(start_y + size) {
                for x in start_x..(start_x + size) {
                    let offset = y * pitch + x * bpp;

                    unsafe {
                        core::ptr::write_volatile(fb_ptr.add(offset), 0xFF);
                        core::ptr::write_volatile(fb_ptr.add(offset + 1), 0xFF);
                        core::ptr::write_volatile(fb_ptr.add(offset + 2), 0xFF);
                        core::ptr::write_volatile(fb_ptr.add(offset + 3), 0xFF);
                    }
                }
            }
        }
    }

    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("dsb sy");
    }

    loop {
        halt();
    }
}

fn halt() {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("hlt");

        #[cfg(target_arch = "aarch64")]
        core::arch::asm!("wfi");
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        halt();
    }
}

#![no_std]
#![no_main]

extern crate alloc;

mod arch;
mod logger;
mod mm;
mod serial;
#[cfg(feature = "kernel-tests")]
mod test;

use alloc::boxed::Box;
use alloc::vec::Vec;

use limine::request::{FramebufferRequest, HhdmRequest, MemmapRequest, StackSizeRequest};
use limine::{BaseRevision, RequestsEndMarker, RequestsStartMarker};

use crate::arch::{Arch, Platform};
use crate::mm::PAGE_SIZE;
use crate::mm::address_space::{AllocKind, KernelAddressSpace};
use crate::mm::heap::LockedHeap;
use crate::mm::paging::PageFlags;

#[global_allocator]
pub static ALLOCATOR: LockedHeap = LockedHeap::new();

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

    let root_phys = Platform::active_page_table();

    let mut kernel_space = unsafe { KernelAddressSpace::from_active(root_phys) };
    let heap_size = 1024 * 1024;

    let heap_region = kernel_space
        .alloc_and_map(
            heap_size,
            PAGE_SIZE,
            PageFlags::kernel_data(),
            AllocKind::Heap,
        )
        .expect("failed to allocate kernel heap");

    unsafe {
        ALLOCATOR
            .lock()
            .add_memory(heap_region.base.as_mut_ptr::<u8>(), heap_region.size);
    }

    log::info!("kernel heap initialised with {} bytes", heap_size);

    let mut test_vec = Vec::new();
    for i in 0..500 {
        test_vec.push(i);
    }

    let test_box = Box::new("eira kernel");

    log::info!("vec length: {}, box: {}", test_vec.len(), test_box);

    #[cfg(feature = "kernel-tests")]
    crate::test::runner::run_all();

    log::info!("halting");
    loop {
        Platform::halt();
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

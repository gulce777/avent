.section .text
.global _start
.type .start, @function
_start:
    // All 16 slots (4 levels x 4 exception types) jump to _exception_hang.
    adr x0, _exception_vectors
    msr vbar_el1, x0
    isb

    // Enable FP/SIMD
    mov x0, #(0b11 << 20)
    msr cpacr_el1, x0
    isb

    // Clear BSS
    adr x0, __bss_start
    adr x1, __bss_end
    bl _clear_bss

    bl kmain

.balign 0x800 // Aligned to 2 KiB
_exception_vectors:
.rept 16
    b _exception_hang
    .balign 0x80
.endr

.global _exception_hang
_exception_hang:
    mrs x0, esr_el1
    mrs x1, elr_el1
    mrs x2, far_el1
    // We read fault address, syndrome etc. into registers for inspection in debugger
    b _exception_hang

.global _halt_forever
_halt_forever:
    msr daifset, #0xf
    wfi
    b _halt_forever

.type _clear_bss, @function
_clear_bss:
    cmp x0, x1
    b.hs .bss_done
.bss_loop:
    strb wzr, [x0], #1
    cmp x0, x1
    b.lo .bss_loop
.bss_done:
    ret

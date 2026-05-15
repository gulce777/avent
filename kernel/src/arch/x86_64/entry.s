.intel_syntax noprefix
.section .text
.global _start
.type _start, @function
_start:
    // Guarantee SP alignment
    and rsp, -16

    // Clear the BSS segment
    // Arguments: rdi = __bss_start, rsi = __bss_end
    lea rdi, [rip + __bss_start]
    lea rsi, [rip + __bss_end]
    call _clear_bss

    call kmain

// Zeroes memory one byte at a time.
// A production kernel would use `rep stosb`? or SIMD, but clarity wins here for now.
.global _halt_forever
_halt_forever:
    cli
    hlt
    jmp _halt_forever

    .type _clear_bss, @function
    _clear_bss:
        cmp rdi, rsi
        jae .done
    .loop:
        mov byte ptr [rdi], 0
        inc rdi
        cmp rdi, rsi
        jb .loop
    .done:
        ret

OUTPUT_FORMAT(elf64-littleaarch64)
OUTPUT_ARCH(aarch64)
ENTRY(_start)

PHDRS
{
    text    PT_LOAD    FLAGS((1 << 0) | (1 << 2));
    rodata  PT_LOAD    FLAGS((1 << 2));
    data    PT_LOAD    FLAGS((1 << 1) | (1 << 2));
}

SECTIONS {
    . = 0xffffffff80000000;

    .text : { *(.text .text.*) } :text

    . = ALIGN(CONSTANT(MAXPAGESIZE));

    .rodata : { *(.rodata .rodata.*) } :rodata

    . = ALIGN(CONSTANT(MAXPAGESIZE));

    .data : {
        *(.data .data.*)

        KEEP(*(.requests_start))
        KEEP(*(.requests))
        KEEP(*(.requests_end))
    } :data

    .bss : {
        . = ALIGN(16);
        PROVIDE(__bss_start = .);
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(16);
        PROVIDE(__bss_end = .);
    } :data

    /DISCARD/ : { *(.eh_frame) *(.note .note.*) }
}

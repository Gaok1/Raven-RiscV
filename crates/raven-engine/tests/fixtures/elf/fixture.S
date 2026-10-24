/*
 * ELF fixture for the loader tests in tests/support/falcon_program_elf.rs
 * and for the ELF tests in the root crate's tests/support/ui_app_internal.rs.
 *
 * It is laid out like a rust-to-raven build: read-only data at the bottom of
 * the image, code on the page above it, then .data and .bss (see fixture.ld).
 * `bump` sits ahead of `_start`, so the entry point is not the first word of
 * the text segment.
 *
 * Run on RAVEN, it prints the greeting, adds one to `counter` and exits with
 * the new value: 41 + 1 = 42. See README.md for how to rebuild fixture.elf.
 */

    .section .rodata
    .globl  greeting
    .type   greeting, @object
greeting:
    .ascii  "hello from the ELF fixture\n"
.Lgreeting_end:
    .size   greeting, .Lgreeting_end - greeting

    .data
    .p2align 2
    .globl  counter
    .type   counter, @object
counter:
    .word   41
    .size   counter, 4

    // Never touched by the code: it gives the data segment a bss tail
    // (p_memsz > p_filesz) that the loader has to zero.
    .bss
    .p2align 2
    .globl  scratch
    .type   scratch, @object
scratch:
    .zero   16
    .size   scratch, 16

    .text
    // bump(a0 = &word): *word += 1. a0 is left unchanged.
    .globl  bump
    .type   bump, @function
bump:
    lw      t0, 0(a0)
    addi    t0, t0, 1
    sw      t0, 0(a0)
    ret
    .size   bump, . - bump

    .globl  _start
    .type   _start, @function
_start:
    li      a0, 1                   // write(1, greeting, len)
    la      a1, greeting
    li      a2, .Lgreeting_end - greeting
    li      a7, 64
    ecall

    la      a0, counter             // counter += 1
    call    bump

    lw      a0, 0(a0)               // exit(counter)
    li      a7, 93
    ecall
    .size   _start, . - _start

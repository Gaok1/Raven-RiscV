# crt0.S — minimal C runtime startup for Raven
#
# Raven already zeroes BSS when loading the ELF, so the only job here is
# to call main() and forward its return value to exit (syscall 93).

    .section .text._start
    .globl   _start

_start:
    call  main          # call user's main(); return value lands in a0
    li    a7, 93        # exit(a0)
    ecall
1:  j     1b            # unreachable — keeps the simulator from running off

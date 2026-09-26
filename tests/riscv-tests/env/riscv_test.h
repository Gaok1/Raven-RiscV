// Minimal environment to run the official riscv-tests suite on RAVEN.
//
// Replaces env/p/riscv_test.h. The official "p" environment initialises the
// registers, parks every hart but hart 0, sets up the trap vector, PMP, satp
// and delegation, enables F and clears fcsr (RVTEST_RV32UF), enters the test
// through mret and signals the end by writing to `tohost`. RAVEN has no HTIF
// host; it ends the simulation through the exit syscall (a7 = 93) and reports
// the exit code on the CLI (`raven run --expect-exit`). So this environment:
//   - starts straight in the test body, with NO initialisation or privileged
//     transition (fcsr is not cleared either; see README, note on F);
//   - keeps RVTEST_PASS and RVTEST_FAIL identical to the official ones: pass
//     is exit(0); fail is exit((TESTNUM << 1) | 1), where TESTNUM is the
//     number of the failing case.
// The tests themselves (isa/rv*/*.S and isa/macros/scalar/test_macros.h) are
// not modified.

#ifndef _ENV_RAVEN_RISCV_TEST_H
#define _ENV_RAVEN_RISCV_TEST_H

#define TESTNUM gp

#define RVTEST_RV64U  .macro init; .endm
#define RVTEST_RV32U  .macro init; .endm
#define RVTEST_RV64UF .macro init; .endm
#define RVTEST_RV32UF .macro init; .endm

#define RVTEST_CODE_BEGIN                                               \
        .section .text.init;                                            \
        .align  6;                                                      \
        .globl _start;                                                  \
_start:                                                                 \
        li TESTNUM, 0;                                                  \
        init;

#define RVTEST_CODE_END                                                 \
        unimp

#define RVTEST_PASS                                                     \
        fence;                                                          \
        li TESTNUM, 1;                                                  \
        li a7, 93;                                                      \
        li a0, 0;                                                       \
        ecall

#define RVTEST_FAIL                                                     \
        fence;                                                          \
1:      beqz TESTNUM, 1b;                                               \
        sll TESTNUM, TESTNUM, 1;                                        \
        or TESTNUM, TESTNUM, 1;                                         \
        li a7, 93;                                                      \
        addi a0, TESTNUM, 0;                                            \
        ecall

#define EXTRA_DATA

#define RVTEST_DATA_BEGIN                                               \
        EXTRA_DATA                                                      \
        .align 4; .global begin_signature; begin_signature:

#define RVTEST_DATA_END                                                 \
        .align 4; .global end_signature; end_signature:

#endif

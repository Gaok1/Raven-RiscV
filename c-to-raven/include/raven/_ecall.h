#ifndef RAVEN__ECALL_H
#define RAVEN__ECALL_H

/* Raven RISC-V ecall numbers.
 *
 * The public wrappers in <raven/io.h>, <raven/mem.h>, etc. cover the common
 * cases. The opt-in <raven/advanced.h> exposes the rest as named raven_sys_*
 * and raven_unsafe_* functions. These constants are public for users who want
 * to issue ecalls themselves via inline assembly. */

/* ── Linux-compatible syscalls ───────────────────────────────────────────── */
#define RAVEN_ECALL_READ            63
#define RAVEN_ECALL_WRITE           64
#define RAVEN_ECALL_WRITEV          66
#define RAVEN_ECALL_EXIT            93
#define RAVEN_ECALL_EXIT_GROUP      94
#define RAVEN_ECALL_GETPID         172
#define RAVEN_ECALL_GETUID         174
#define RAVEN_ECALL_GETGID         176
#define RAVEN_ECALL_BRK            214
#define RAVEN_ECALL_MUNMAP         215
#define RAVEN_ECALL_MMAP           222
#define RAVEN_ECALL_GETRANDOM      278
#define RAVEN_ECALL_CLOCK_GETTIME  403

/* ── Raven teaching extensions ───────────────────────────────────────────── */
#define RAVEN_ECALL_PRINT_INT        1000
#define RAVEN_ECALL_PRINT_STR        1001
#define RAVEN_ECALL_PRINTLN_STR      1002
#define RAVEN_ECALL_READ_LINE        1003
#define RAVEN_ECALL_PRINT_UINT       1004
#define RAVEN_ECALL_PRINT_HEX        1005
#define RAVEN_ECALL_PRINT_CHAR       1006
#define RAVEN_ECALL_PRINT_NEWLINE    1008
#define RAVEN_ECALL_READ_U8          1010
#define RAVEN_ECALL_READ_U16         1011
#define RAVEN_ECALL_READ_U32         1012
#define RAVEN_ECALL_READ_INT         1013
#define RAVEN_ECALL_READ_FLOAT       1014
#define RAVEN_ECALL_PRINT_FLOAT      1015
#define RAVEN_ECALL_INSTR_COUNT      1030
#define RAVEN_ECALL_CYCLE_COUNT      1031
#define RAVEN_ECALL_MEMSET           1050
#define RAVEN_ECALL_MEMCPY           1051
#define RAVEN_ECALL_STRLEN           1052
#define RAVEN_ECALL_STRCMP           1053
#define RAVEN_ECALL_HART_START       1100
#define RAVEN_ECALL_HART_EXIT        1101
#define RAVEN_ECALL_MAP_EXEC         1102

#endif

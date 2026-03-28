#pragma once

// ─────────────────────────────────────────────────────────────────────────────
// raven.h  —  bare-metal runtime for RISC-V programs running in Raven
//
// No libc, no OS. Everything here is self-contained static inline.
// Include once in your .c file and you get syscalls, I/O, strings,
// memory utilities, a heap allocator, and simulator control.
// ─────────────────────────────────────────────────────────────────────────────

// ── Syscall numbers ──────────────────────────────────────────────────────────
#define SYS_READ          63
#define SYS_WRITE         64
#define SYS_EXIT          93
#define SYS_EXIT_GROUP    94
#define SYS_WRITEV        66
#define SYS_GETPID        172
#define SYS_GETUID        174
#define SYS_GETGID        176
#define SYS_BRK           214
#define SYS_MUNMAP        215
#define SYS_MMAP          222
#define SYS_GETRANDOM     278
#define SYS_CLOCK_GETTIME 403
#define SYS_RAVEN_PRINT_INT        1000
#define SYS_RAVEN_PRINT_STR        1001
#define SYS_RAVEN_PRINTLN_STR      1002
#define SYS_RAVEN_READ_LINE        1003
#define SYS_RAVEN_PRINT_UINT       1004
#define SYS_RAVEN_PRINT_HEX        1005
#define SYS_RAVEN_PRINT_CHAR       1006
#define SYS_RAVEN_PRINT_NEWLINE    1008
#define SYS_RAVEN_READ_U8          1010
#define SYS_RAVEN_READ_U16         1011
#define SYS_RAVEN_READ_U32         1012
#define SYS_RAVEN_READ_INT         1013
#define SYS_RAVEN_READ_FLOAT       1014
#define SYS_RAVEN_PRINT_FLOAT      1015
#define SYS_RAVEN_GET_INSTR_COUNT  1030
#define SYS_RAVEN_GET_CYCLE_COUNT  1031
#define SYS_RAVEN_MEMSET           1050
#define SYS_RAVEN_MEMCPY           1051
#define SYS_RAVEN_STRLEN           1052
#define SYS_RAVEN_STRCMP           1053
#define SYS_RAVEN_HART_START       1100
#define SYS_RAVEN_HART_EXIT        1101

// ── mmap flags / prot ────────────────────────────────────────────────────────
#define PROT_NONE     0x00
#define PROT_READ     0x01
#define PROT_WRITE    0x02
#define PROT_EXEC     0x04
#define MAP_SHARED    0x01
#define MAP_PRIVATE   0x02
#define MAP_ANONYMOUS 0x20
#define MAP_ANON      MAP_ANONYMOUS

// ── File descriptors ─────────────────────────────────────────────────────────
#define STDIN  0
#define STDOUT 1
#define STDERR 2

// ── NULL ─────────────────────────────────────────────────────────────────────
#ifndef NULL
#define NULL ((void *)0)
#endif

// ── Types ────────────────────────────────────────────────────────────────────
typedef unsigned int   size_t;
typedef int            ptrdiff_t;
typedef unsigned long long raven_u64;

// Assert: if expr is false, print message and halt.
#define raven_assert(expr) \
    do { if (!(expr)) raven_panic("assertion failed: " #expr); } while (0)

// ─────────────────────────────────────────────────────────────────────────────
// HEAP ALLOCATOR
//
// A simple first-fit free-list allocator backed by a static 64 KB heap.
// Great for watching malloc/free in Raven's Dyn view: every `sw` that
// writes a block header is visible in real time.
//
// Change RAVEN_HEAP_SIZE before including this header to resize the heap.
// ─────────────────────────────────────────────────────────────────────────────

#include "internal/raven_syscall.h"
#include "internal/raven_libc.h"
#include "internal/raven_heap.h"
#include "internal/raven_teaching.h"


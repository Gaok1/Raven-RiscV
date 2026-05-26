#ifndef RAVEN_IO_H
#define RAVEN_IO_H

#include <raven/types.h>
#include <raven/_ecall.h>

/* ── Output (each is one ecall — formatting cost does NOT count toward
 * instr_count, so benchmarks stay clean) ─────────────────────────────────── */

static inline void raven_print_int(int n) {
    register int _a7 __asm__("a7") = RAVEN_ECALL_PRINT_INT;
    register int _a0 __asm__("a0") = n;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_print_uint(raven_u32 n) {
    register int       _a7 __asm__("a7") = RAVEN_ECALL_PRINT_UINT;
    register raven_u32 _a0 __asm__("a0") = n;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_print_hex(raven_u32 n) {
    register int       _a7 __asm__("a7") = RAVEN_ECALL_PRINT_HEX;
    register raven_u32 _a0 __asm__("a0") = n;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_print_str(const char *s) {
    register int         _a7 __asm__("a7") = RAVEN_ECALL_PRINT_STR;
    register const char *_a0 __asm__("a0") = s;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_println_str(const char *s) {
    register int         _a7 __asm__("a7") = RAVEN_ECALL_PRINTLN_STR;
    register const char *_a0 __asm__("a0") = s;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_print_char(char c) {
    register int _a7 __asm__("a7") = RAVEN_ECALL_PRINT_CHAR;
    register int _a0 __asm__("a0") = (raven_u8)c;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

/* Print a newline (just '\n'). */
static inline void raven_println(void) {
    register int _a7 __asm__("a7") = RAVEN_ECALL_PRINT_NEWLINE;
    __asm__ volatile("ecall" :: "r"(_a7));
}

/* Default precision (6 significant digits, trailing zeros stripped — matches
 * the Rust simulator's formatter). For custom precision use
 * raven_print_float_n(). */
static inline void raven_print_float(float v) {
    union { float f; raven_u32 bits; } u; u.f = v;
    register int       _a7 __asm__("a7") = RAVEN_ECALL_PRINT_FLOAT;
    register raven_u32 _a0 __asm__("a0") = u.bits;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_print_ptr(const void *p) {
    raven_print_hex((raven_u32)(raven_uintptr_t)p);
}

static inline void raven_print_bool(int v) {
    raven_print_str(v ? "true" : "false");
}

static inline void raven_print_bin(raven_u32 n) {
    for (int i = 31; i >= 0; i--) {
        raven_print_char('0' + (char)((n >> i) & 1));
        if (i > 0 && i % 8 == 0) raven_print_char(' ');
    }
}

/* Custom-precision float printing. The formatting work happens in C and
 * therefore counts toward instr_count. Defined in libraven.a. */
void raven_print_float_n(float v, int decimals);

/* ── Stderr (defined in libraven.a — fd=2 is not exposed publicly) ──────── */
void raven_eprint_str (const char *s);
void raven_eprint_int (int n);
void raven_eprint_uint(raven_u32 n);
void raven_eprint_char(char c);
void raven_eprintln   (void);

/* ── Input ───────────────────────────────────────────────────────────────── */

static inline raven_u8 raven_read_u8(void) {
    raven_u8 v;
    register int       _a7 __asm__("a7") = RAVEN_ECALL_READ_U8;
    register raven_u8 *_a0 __asm__("a0") = &v;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0) : "memory");
    return v;
}

static inline raven_u16 raven_read_u16(void) {
    raven_u16 v;
    register int        _a7 __asm__("a7") = RAVEN_ECALL_READ_U16;
    register raven_u16 *_a0 __asm__("a0") = &v;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0) : "memory");
    return v;
}

static inline raven_u32 raven_read_uint(void) {
    raven_u32 v;
    register int        _a7 __asm__("a7") = RAVEN_ECALL_READ_U32;
    register raven_u32 *_a0 __asm__("a0") = &v;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0) : "memory");
    return v;
}

#define raven_read_u32 raven_read_uint

static inline int raven_read_int(void) {
    int v;
    register int  _a7 __asm__("a7") = RAVEN_ECALL_READ_INT;
    register int *_a0 __asm__("a0") = &v;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0) : "memory");
    return v;
}

static inline float raven_read_float(void) {
    float v;
    register int    _a7 __asm__("a7") = RAVEN_ECALL_READ_FLOAT;
    register float *_a0 __asm__("a0") = &v;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0) : "memory");
    return v;
}

/* Bounded line read from stdin. Reads up to (max-1) bytes until newline or
 * EOF, then NUL-terminates buf. Returns byte count written (excluding NUL).
 * Defined in libraven.a — implemented as a byte-at-a-time loop on the read
 * ecall because the simulator's line-read ecall is unbounded and unsafe. */
int raven_read_line(char *buf, int max);

/* Single character from stdin. Returns -1 on EOF. */
int raven_read_char(void);

#endif

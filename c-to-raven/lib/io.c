#include <raven/io.h>
#include <raven/_ecall.h>

/* Private helpers: direct Linux ecalls. The public stderr fd is intentionally
 * not exposed in <raven/io.h>; callers who need fd-based I/O go through
 * <raven/advanced.h>. */

static int _raven_write(int fd, const void *buf, int len) {
    register int         _a7 __asm__("a7") = RAVEN_ECALL_WRITE;
    register int         _a0 __asm__("a0") = fd;
    register const void *_a1 __asm__("a1") = buf;
    register int         _a2 __asm__("a2") = len;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret)
                              : "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2)
                              : "memory");
    return ret;
}

static int _raven_read(int fd, void *buf, int len) {
    register int   _a7 __asm__("a7") = RAVEN_ECALL_READ;
    register int   _a0 __asm__("a0") = fd;
    register void *_a1 __asm__("a1") = buf;
    register int   _a2 __asm__("a2") = len;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret)
                              : "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2)
                              : "memory");
    return ret;
}

static raven_size_t _raven_strlen_local(const char *s) {
    raven_size_t n = 0; while (*s++) n++; return n;
}

#define _RAVEN_STDERR 2

/* ── Stderr family ───────────────────────────────────────────────────────── */

void raven_eprint_str(const char *s) {
    _raven_write(_RAVEN_STDERR, s, (int)_raven_strlen_local(s));
}

void raven_eprint_char(char c) {
    _raven_write(_RAVEN_STDERR, &c, 1);
}

void raven_eprintln(void) {
    char nl = '\n';
    _raven_write(_RAVEN_STDERR, &nl, 1);
}

void raven_eprint_uint(raven_u32 n) {
    char buf[12]; int i = 11; buf[i] = '\0';
    if (n == 0) { raven_eprint_char('0'); return; }
    while (n > 0) { buf[--i] = (char)('0' + (char)(n % 10)); n /= 10; }
    raven_eprint_str(buf + i);
}

void raven_eprint_int(int n) {
    if (n < 0) { raven_eprint_char('-'); raven_eprint_uint((raven_u32)(-n)); }
    else        { raven_eprint_uint((raven_u32)n); }
}

/* ── Custom-precision float printing ─────────────────────────────────────── */

void raven_print_float_n(float v, int decimals) {
    if (v < 0.0f) { raven_print_char('-'); v = -v; }
    raven_u32 whole = (raven_u32)v;
    raven_print_uint(whole);
    if (decimals > 0) {
        raven_print_char('.');
        float frac = v - (float)whole;
        while (decimals--) {
            frac *= 10.0f;
            int d = (int)frac;
            raven_print_char((char)('0' + (char)d));
            frac -= (float)d;
        }
    }
}

/* ── Input ──────────────────────────────────────────────────────────────── */

int raven_read_line(char *buf, int max) {
    if (max <= 0) return 0;
    int n = 0;
    while (n < max - 1) {
        char c;
        int r = _raven_read(0, &c, 1);
        if (r <= 0 || c == '\n') break;
        buf[n++] = c;
    }
    buf[n] = '\0';
    return n;
}

int raven_read_char(void) {
    raven_u8 c;
    int n = _raven_read(0, &c, 1);
    return n > 0 ? (int)c : -1;
}

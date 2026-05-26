#include <raven/fmt.h>
#include <raven/io.h>
#include <raven/_ecall.h>
#include <stdarg.h>

/* ── Private write/read ecall helpers (mirror io.c so we don't pull in
 *    its file-static functions; keeps each TU self-contained). */

static int _fmt_write(int fd, const void *buf, int len) {
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

static int _fmt_read1(char *out) {
    register int   _a7 __asm__("a7") = RAVEN_ECALL_READ;
    register int   _a0 __asm__("a0") = 0;
    register void *_a1 __asm__("a1") = out;
    register int   _a2 __asm__("a2") = 1;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret)
                              : "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2)
                              : "memory");
    return ret;
}

/* ── Local character classes (don't depend on a libc <ctype.h>) ─────────── */

static int _is_digit (int c) { return c >= '0' && c <= '9'; }
static int _is_odigit(int c) { return c >= '0' && c <= '7'; }
static int _is_xdigit(int c) {
    return (c >= '0' && c <= '9') ||
           (c >= 'a' && c <= 'f') ||
           (c >= 'A' && c <= 'F');
}
static int _is_space (int c) {
    return c == ' ' || c == '\t' || c == '\n' ||
           c == '\r' || c == '\v' || c == '\f';
}
static int _xdigit_val(int c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    return c - 'A' + 10;
}

/* ── Output writer abstraction ──────────────────────────────────────────── */
/* Two modes:
 *   - stdout: append to a 128-byte batch buffer, flush to fd=1 on overflow
 *     or at end-of-format. Keeps the ecall count low.
 *   - user buffer: write directly into the caller's buffer, respecting
 *     bounds so we can implement snprintf semantics. */

#define _RAVEN_FMT_BATCH 128

typedef struct {
    int          to_stdout;
    char        *user_buf;
    raven_size_t user_cap;
    raven_size_t user_pos;
    raven_size_t total;
    raven_size_t sb_pos;
    char         stdout_buf[_RAVEN_FMT_BATCH];
} _Writer;

static void _w_flush(_Writer *w) {
    if (w->to_stdout && w->sb_pos > 0) {
        _fmt_write(1, w->stdout_buf, (int)w->sb_pos);
        w->sb_pos = 0;
    }
}

static void _w_putc(_Writer *w, char c) {
    if (w->to_stdout) {
        if (w->sb_pos >= _RAVEN_FMT_BATCH) _w_flush(w);
        w->stdout_buf[w->sb_pos++] = c;
    } else {
        if (w->user_cap > 0 && w->user_pos + 1 < w->user_cap) {
            w->user_buf[w->user_pos++] = c;
        }
    }
    w->total++;
}

static void _w_buf(_Writer *w, const char *s, int n) {
    for (int i = 0; i < n; i++) _w_putc(w, s[i]);
}

static void _w_pad(_Writer *w, char c, int n) {
    while (n-- > 0) _w_putc(w, c);
}

/* ── Integer → ASCII ────────────────────────────────────────────────────── */
/* buf must hold ≥ 33 bytes (32 binary digits + NUL terminator never written). */
static int _utoa(raven_u32 v, unsigned base, int upper, char *buf) {
    static const char ld[] = "0123456789abcdef";
    static const char ud[] = "0123456789ABCDEF";
    const char *digs = upper ? ud : ld;
    int i = 0;
    if (v == 0) { buf[i++] = '0'; return i; }
    while (v) { buf[i++] = digs[v % base]; v /= base; }
    for (int a = 0, b = i - 1; a < b; a++, b--) {
        char t = buf[a]; buf[a] = buf[b]; buf[b] = t;
    }
    return i;
}

/* Flag bits */
#define _F_LEFT  (1 << 0)
#define _F_PLUS  (1 << 1)
#define _F_SPACE (1 << 2)
#define _F_ZERO  (1 << 3)
#define _F_HASH  (1 << 4)

static void _emit_int(_Writer *w, const char *digits, int dlen,
                      const char *prefix, int plen, int flags,
                      int width, int precision) {
    int zeros = (precision > dlen) ? (precision - dlen) : 0;
    int body  = plen + zeros + dlen;
    int pad   = (width > body) ? (width - body) : 0;
    int zero_pad = (flags & _F_ZERO) && !(flags & _F_LEFT) && precision < 0;

    if (zero_pad) {
        _w_buf(w, prefix, plen);
        _w_pad(w, '0', pad + zeros);
        _w_buf(w, digits, dlen);
    } else if (flags & _F_LEFT) {
        _w_buf(w, prefix, plen);
        _w_pad(w, '0', zeros);
        _w_buf(w, digits, dlen);
        _w_pad(w, ' ', pad);
    } else {
        _w_pad(w, ' ', pad);
        _w_buf(w, prefix, plen);
        _w_pad(w, '0', zeros);
        _w_buf(w, digits, dlen);
    }
}

/* ── Core printf engine ─────────────────────────────────────────────────── */

static int _vfmt(_Writer *w, const char *fmt, va_list ap) {
    char dbuf[34];

    for (; *fmt; fmt++) {
        if (*fmt != '%') { _w_putc(w, *fmt); continue; }
        fmt++;

        /* flags */
        int flags = 0;
        for (;;) {
            switch (*fmt) {
            case '-': flags |= _F_LEFT;  fmt++; continue;
            case '+': flags |= _F_PLUS;  fmt++; continue;
            case ' ': flags |= _F_SPACE; fmt++; continue;
            case '0': flags |= _F_ZERO;  fmt++; continue;
            case '#': flags |= _F_HASH;  fmt++; continue;
            }
            break;
        }

        /* width */
        int width = 0;
        while (_is_digit(*fmt)) { width = width * 10 + (*fmt - '0'); fmt++; }

        /* precision */
        int precision = -1;
        if (*fmt == '.') {
            fmt++;
            precision = 0;
            while (_is_digit(*fmt)) { precision = precision * 10 + (*fmt - '0'); fmt++; }
        }

        /* length modifiers — int/long are both 32-bit on rv32, so we accept
         * and ignore them. */
        while (*fmt == 'h' || *fmt == 'l' || *fmt == 'z' ||
               *fmt == 'j' || *fmt == 't' || *fmt == 'L') fmt++;

        switch (*fmt) {
        case 'd': case 'i': {
            int v = va_arg(ap, int);
            raven_u32 u; const char *pfx = ""; int plen = 0;
            if (v < 0) {
                /* avoid INT_MIN overflow when negating */
                u = (raven_u32)0 - (raven_u32)v;
                pfx = "-"; plen = 1;
            } else {
                u = (raven_u32)v;
                if (flags & _F_PLUS)       { pfx = "+"; plen = 1; }
                else if (flags & _F_SPACE) { pfx = " "; plen = 1; }
            }
            int dlen = _utoa(u, 10, 0, dbuf);
            if (precision == 0 && u == 0) dlen = 0;
            _emit_int(w, dbuf, dlen, pfx, plen, flags, width, precision);
            break;
        }
        case 'u': {
            raven_u32 u = (raven_u32)va_arg(ap, unsigned int);
            int dlen = _utoa(u, 10, 0, dbuf);
            if (precision == 0 && u == 0) dlen = 0;
            _emit_int(w, dbuf, dlen, "", 0, flags, width, precision);
            break;
        }
        case 'x': case 'X': {
            raven_u32 u = (raven_u32)va_arg(ap, unsigned int);
            int up = (*fmt == 'X');
            int dlen = _utoa(u, 16, up, dbuf);
            if (precision == 0 && u == 0) dlen = 0;
            const char *pfx = ""; int plen = 0;
            if ((flags & _F_HASH) && u != 0) { pfx = up ? "0X" : "0x"; plen = 2; }
            _emit_int(w, dbuf, dlen, pfx, plen, flags, width, precision);
            break;
        }
        case 'o': {
            raven_u32 u = (raven_u32)va_arg(ap, unsigned int);
            int dlen = _utoa(u, 8, 0, dbuf);
            if (precision == 0 && u == 0) dlen = 0;
            const char *pfx = ""; int plen = 0;
            if ((flags & _F_HASH) && u != 0) { pfx = "0"; plen = 1; }
            _emit_int(w, dbuf, dlen, pfx, plen, flags, width, precision);
            break;
        }
        case 'b': {
            raven_u32 u = (raven_u32)va_arg(ap, unsigned int);
            int dlen = _utoa(u, 2, 0, dbuf);
            if (precision == 0 && u == 0) dlen = 0;
            _emit_int(w, dbuf, dlen, "", 0, flags, width, precision);
            break;
        }
        case 'c': {
            char ch = (char)va_arg(ap, int);
            int pad = width > 1 ? width - 1 : 0;
            if (!(flags & _F_LEFT)) _w_pad(w, ' ', pad);
            _w_putc(w, ch);
            if (flags & _F_LEFT)    _w_pad(w, ' ', pad);
            break;
        }
        case 's': {
            const char *s = va_arg(ap, const char *);
            if (!s) s = "(null)";
            raven_size_t slen = 0;
            while (s[slen] && (precision < 0 || (int)slen < precision)) slen++;
            int pad = width > (int)slen ? width - (int)slen : 0;
            if (!(flags & _F_LEFT)) _w_pad(w, ' ', pad);
            _w_buf(w, s, (int)slen);
            if (flags & _F_LEFT)    _w_pad(w, ' ', pad);
            break;
        }
        case 'p': {
            raven_u32 u = (raven_u32)(raven_uintptr_t)va_arg(ap, void *);
            int dlen = _utoa(u, 16, 0, dbuf);
            int p = precision < 0 ? 8 : precision;
            _emit_int(w, dbuf, dlen, "0x", 2, flags, width, p);
            break;
        }
        case '%':
            _w_putc(w, '%');
            break;
        case '\0':
            /* trailing '%' — rewind so the outer loop's increment exits. */
            fmt--;
            break;
        default:
            /* unknown conversion: emit the literal "%X" */
            _w_putc(w, '%');
            _w_putc(w, *fmt);
            break;
        }
    }
    return (int)w->total;
}

/* ── Public printf / snprintf ───────────────────────────────────────────── */

int raven_vprintf(const char *fmt, raven_va_list ap) {
    _Writer w;
    w.to_stdout = 1;
    w.user_buf = (char *)0;
    w.user_cap = 0;
    w.user_pos = 0;
    w.total    = 0;
    w.sb_pos   = 0;
    int n = _vfmt(&w, fmt, ap);
    _w_flush(&w);
    return n;
}

int raven_printf(const char *fmt, ...) {
    va_list ap; va_start(ap, fmt);
    int n = raven_vprintf(fmt, ap);
    va_end(ap);
    return n;
}

int raven_vsnprintf(char *buf, raven_size_t size,
                    const char *fmt, raven_va_list ap) {
    _Writer w;
    w.to_stdout = 0;
    w.user_buf  = buf;
    w.user_cap  = size;
    w.user_pos  = 0;
    w.total     = 0;
    w.sb_pos    = 0;
    int n = _vfmt(&w, fmt, ap);
    if (size > 0) {
        raven_size_t term = w.user_pos < size ? w.user_pos : size - 1;
        buf[term] = '\0';
    }
    return n;
}

int raven_snprintf(char *buf, raven_size_t size, const char *fmt, ...) {
    va_list ap; va_start(ap, fmt);
    int n = raven_vsnprintf(buf, size, fmt, ap);
    va_end(ap);
    return n;
}

/* ── Input reader abstraction ───────────────────────────────────────────── */
/* One-character lookahead, since stdin has no ungetc. For sscanf the source
 * is a NUL-terminated string; for scanf the source is fd=0. */

typedef struct {
    const char  *s;          /* NULL ⇒ read from stdin */
    raven_size_t pos;
    int          has_peek;
    int          peek_c;
} _Reader;

static int _r_peek(_Reader *r) {
    if (r->has_peek) return r->peek_c;
    int c;
    if (r->s) {
        char ch = r->s[r->pos];
        if (ch == '\0') return -1;
        r->pos++;
        c = (int)(unsigned char)ch;
    } else {
        char ch;
        int n = _fmt_read1(&ch);
        if (n <= 0) return -1;
        c = (int)(unsigned char)ch;
    }
    r->has_peek = 1;
    r->peek_c   = c;
    return c;
}

static void _r_consume(_Reader *r) { r->has_peek = 0; }

/* ── Core scanf engine ──────────────────────────────────────────────────── */

static int _vscan(_Reader *r, const char *fmt, va_list ap) {
    int matches = 0;

    while (*fmt) {
        unsigned char fc = (unsigned char)*fmt;

        if (_is_space(fc)) {
            int c;
            while ((c = _r_peek(r)) >= 0 && _is_space(c)) _r_consume(r);
            fmt++;
            continue;
        }

        if (fc != '%') {
            int c = _r_peek(r);
            if (c < 0 || c != (int)fc) return matches;
            _r_consume(r);
            fmt++;
            continue;
        }

        /* conversion specifier */
        fmt++;

        int suppress = 0;
        if (*fmt == '*') { suppress = 1; fmt++; }

        int width = 0, has_width = 0;
        while (_is_digit((unsigned char)*fmt)) {
            width = width * 10 + (*fmt - '0');
            has_width = 1;
            fmt++;
        }

        while (*fmt == 'h' || *fmt == 'l' || *fmt == 'z' ||
               *fmt == 'j' || *fmt == 't' || *fmt == 'L') fmt++;

        char conv = *fmt;
        if (!conv) break;
        fmt++;

        switch (conv) {

        case 'd': case 'i': case 'u': case 'x': case 'X': case 'o': case 'b': {
            int c;
            while ((c = _r_peek(r)) >= 0 && _is_space(c)) _r_consume(r);

            int base =
                (conv == 'd' || conv == 'i' || conv == 'u') ? 10 :
                (conv == 'o') ? 8 :
                (conv == 'b') ? 2 : 16;
            int allow_sign = (conv == 'd' || conv == 'i');
            int neg = 0, cnt = 0;

            c = _r_peek(r);
            if (c < 0) return matches;

            if (allow_sign && (c == '-' || c == '+')) {
                if (c == '-') neg = 1;
                _r_consume(r);
                cnt++;
                if (has_width && cnt >= width) return matches;
                c = _r_peek(r);
            }

            /* %i / %x accept an optional 0x prefix */
            if ((conv == 'i' || conv == 'x' || conv == 'X') && c == '0') {
                _r_consume(r); cnt++;
                int c2 = _r_peek(r);
                if (c2 == 'x' || c2 == 'X') {
                    _r_consume(r); cnt++;
                    base = 16;
                } else {
                    /* "0" already counts as a valid digit; if %i, switch to octal. */
                    if (conv == 'i') base = 8;
                    raven_u32 val = 0;
                    while ((c = _r_peek(r)) >= 0) {
                        int d;
                        if (base == 16) {
                            if (!_is_xdigit(c)) break;
                            d = _xdigit_val(c);
                        } else if (base == 8) {
                            if (!_is_odigit(c)) break;
                            d = c - '0';
                        } else if (base == 2) {
                            if (c != '0' && c != '1') break;
                            d = c - '0';
                        } else {
                            if (!_is_digit(c)) break;
                            d = c - '0';
                        }
                        val = val * (unsigned)base + (unsigned)d;
                        _r_consume(r);
                        cnt++;
                        if (has_width && cnt >= width) break;
                    }
                    if (!suppress) {
                        if (conv == 'd' || conv == 'i') {
                            int *p = va_arg(ap, int *);
                            *p = neg ? -(int)val : (int)val;
                        } else {
                            raven_u32 *p = va_arg(ap, raven_u32 *);
                            *p = val;
                        }
                        matches++;
                    }
                    continue;
                }
            }

            raven_u32 val = 0;
            int digits = 0;
            while ((c = _r_peek(r)) >= 0) {
                int d;
                if (base == 16) {
                    if (!_is_xdigit(c)) break;
                    d = _xdigit_val(c);
                } else if (base == 8) {
                    if (!_is_odigit(c)) break;
                    d = c - '0';
                } else if (base == 2) {
                    if (c != '0' && c != '1') break;
                    d = c - '0';
                } else {
                    if (!_is_digit(c)) break;
                    d = c - '0';
                }
                val = val * (unsigned)base + (unsigned)d;
                _r_consume(r);
                digits++;
                cnt++;
                if (has_width && cnt >= width) break;
            }
            if (digits == 0) return matches;
            if (!suppress) {
                if (conv == 'd' || conv == 'i') {
                    int *p = va_arg(ap, int *);
                    *p = neg ? -(int)val : (int)val;
                } else {
                    raven_u32 *p = va_arg(ap, raven_u32 *);
                    *p = val;
                }
                matches++;
            }
            break;
        }

        case 'c': {
            int n = has_width ? width : 1;
            char *p = suppress ? (char *)0 : va_arg(ap, char *);
            int got = 0;
            for (int i = 0; i < n; i++) {
                int c = _r_peek(r);
                if (c < 0) break;
                if (!suppress) p[i] = (char)c;
                _r_consume(r);
                got++;
            }
            if (got == 0) return matches;
            if (!suppress) matches++;
            break;
        }

        case 's': {
            int c;
            while ((c = _r_peek(r)) >= 0 && _is_space(c)) _r_consume(r);
            char *p = suppress ? (char *)0 : va_arg(ap, char *);
            int i = 0;
            while ((c = _r_peek(r)) >= 0 && !_is_space(c)) {
                if (has_width && i >= width) break;
                if (!suppress) p[i] = (char)c;
                _r_consume(r);
                i++;
            }
            if (i == 0) return matches;
            if (!suppress) { p[i] = '\0'; matches++; }
            break;
        }

        case '%': {
            int c = _r_peek(r);
            if (c != '%') return matches;
            _r_consume(r);
            break;
        }

        default:
            /* unknown specifier — bail out, returning conversions so far */
            return matches;
        }
    }
    return matches;
}

/* ── Public scanf family ────────────────────────────────────────────────── */

int raven_vsscanf(const char *str, const char *fmt, raven_va_list ap) {
    _Reader r;
    r.s = str; r.pos = 0; r.has_peek = 0; r.peek_c = 0;
    return _vscan(&r, fmt, ap);
}

int raven_sscanf(const char *str, const char *fmt, ...) {
    va_list ap; va_start(ap, fmt);
    int n = raven_vsscanf(str, fmt, ap);
    va_end(ap);
    return n;
}

int raven_vscanf(const char *fmt, raven_va_list ap) {
    _Reader r;
    r.s = (const char *)0; r.pos = 0; r.has_peek = 0; r.peek_c = 0;
    return _vscan(&r, fmt, ap);
}

int raven_scanf(const char *fmt, ...) {
    va_list ap; va_start(ap, fmt);
    int n = raven_vscanf(fmt, ap);
    va_end(ap);
    return n;
}

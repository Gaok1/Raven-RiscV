#pragma once

// Internal libc-like helper layer for raven.h.

static inline void *memset(void *dst, int c, size_t n) {
    unsigned char *p = (unsigned char *)dst;
    while (n--) *p++ = (unsigned char)c;
    return dst;
}

static inline void *memcpy(void *dst, const void *src, size_t n) {
    unsigned char       *d = (unsigned char *)dst;
    const unsigned char *s = (const unsigned char *)src;
    while (n--) *d++ = *s++;
    return dst;
}

static inline void *memmove(void *dst, const void *src, size_t n) {
    unsigned char       *d = (unsigned char *)dst;
    const unsigned char *s = (const unsigned char *)src;
    if (d < s) {
        while (n--) *d++ = *s++;
    } else if (d > s) {
        d += n; s += n;
        while (n--) *--d = *--s;
    }
    return dst;
}

static inline int memcmp(const void *a, const void *b, size_t n) {
    const unsigned char *p = (const unsigned char *)a;
    const unsigned char *q = (const unsigned char *)b;
    while (n--) {
        if (*p != *q) return (int)*p - (int)*q;
        p++; q++;
    }
    return 0;
}

static inline size_t strlen(const char *s) {
    size_t n = 0;
    while (*s++) n++;
    return n;
}

static inline int strcmp(const char *a, const char *b) {
    while (*a && *a == *b) { a++; b++; }
    return (unsigned char)*a - (unsigned char)*b;
}

static inline int strncmp(const char *a, const char *b, size_t n) {
    while (n-- && *a && *a == *b) { a++; b++; }
    if (n == (size_t)-1) return 0;
    return (unsigned char)*a - (unsigned char)*b;
}

static inline char *strcpy(char *dst, const char *src) {
    char *d = dst;
    while ((*d++ = *src++));
    return dst;
}

static inline char *strncpy(char *dst, const char *src, size_t n) {
    char *d = dst;
    while (n && (*d++ = *src++)) n--;
    while (n--) *d++ = '\0';
    return dst;
}

static inline char *strcat(char *dst, const char *src) {
    char *d = dst;
    while (*d) d++;
    while ((*d++ = *src++));
    return dst;
}

static inline char *strchr(const char *s, int c) {
    while (*s) {
        if (*s == (char)c) return (char *)s;
        s++;
    }
    return (c == '\0') ? (char *)s : NULL;
}

static inline char *strrchr(const char *s, int c) {
    const char *last = NULL;
    do {
        if (*s == (char)c) last = s;
    } while (*s++);
    return (char *)last;
}

static inline unsigned int rand_u32(void) {
    unsigned int v;
    __sys_getrandom(&v, (int)sizeof(v), 0);
    return v;
}

static inline unsigned char rand_u8(void) {
    unsigned char v;
    __sys_getrandom(&v, 1, 0);
    return v;
}

static inline unsigned int rand_range(unsigned int lo, unsigned int hi) {
    if (hi <= lo) return lo;
    return lo + rand_u32() % (hi - lo);
}

static inline int rand_i32(void) {
    return (int)rand_u32();
}

static inline int rand_bool(void) {
    return (int)(rand_u8() & 1u);
}

static inline int abs(int n) { return n < 0 ? -n : n; }
static inline int min(int a, int b) { return a < b ? a : b; }
static inline int max(int a, int b) { return a > b ? a : b; }
static inline unsigned int umin(unsigned int a, unsigned int b) { return a < b ? a : b; }
static inline unsigned int umax(unsigned int a, unsigned int b) { return a > b ? a : b; }

static inline int ipow(int base, unsigned int exp) {
    int result = 1;
    while (exp--) result *= base;
    return result;
}

static inline void print_char(char c) {
    __sys_write(STDOUT, &c, 1);
}

static inline void print_str(const char *s) {
    __sys_write(STDOUT, s, (int)strlen(s));
}

static inline void print_ln(void) { print_char('\n'); }
static inline void print_newline(void) { print_char('\n'); }

static inline void print_uint(unsigned int n) {
    char buf[12];
    int i = 11;
    buf[i] = '\0';
    if (n == 0) { print_char('0'); return; }
    while (n > 0) { buf[--i] = '0' + (char)(n % 10); n /= 10; }
    print_str(buf + i);
}

static inline void print_int(int n) {
    if (n < 0) { print_char('-'); print_uint((unsigned int)(-n)); }
    else { print_uint((unsigned int)n); }
}

static inline void print_hex(unsigned int n) {
    const char *hex = "0123456789abcdef";
    char buf[11];
    buf[0] = '0'; buf[1] = 'x';
    for (int i = 9; i >= 2; i--) {
        buf[i] = hex[n & 0xF];
        n >>= 4;
    }
    buf[10] = '\0';
    print_str(buf);
}

static inline void print_ptr(const void *p) {
    print_hex((unsigned int)(size_t)p);
}

static inline void print_float(float v, int decimals) {
    if (v < 0.0f) { print_char('-'); v = -v; }
    unsigned int i = (unsigned int)v;
    print_uint(i);
    if (decimals > 0) {
        print_char('.');
        float frac = v - (float)i;
        while (decimals--) {
            frac *= 10.0f;
            int d = (int)frac;
            print_char('0' + (char)d);
            frac -= (float)d;
        }
    }
}

static inline void print_bool(int v) {
    print_str(v ? "true" : "false");
}

static inline int read_char(void) {
    unsigned char c;
    int n = __sys_read(STDIN, &c, 1);
    return n > 0 ? (int)c : -1;
}

static inline int read_line(char *buf, int max) {
    int n = 0;
    while (n < max - 1) {
        char c;
        if (__sys_read(STDIN, &c, 1) <= 0 || c == '\n') break;
        buf[n++] = c;
    }
    buf[n] = '\0';
    return n;
}

static inline int read_int(void) {
    char buf[24];
    read_line(buf, sizeof(buf));
    int sign = 1, i = 0, result = 0;
    if (buf[i] == '-') { sign = -1; i++; }
    for (; buf[i] >= '0' && buf[i] <= '9'; i++) {
        result = result * 10 + (buf[i] - '0');
    }
    return sign * result;
}

static inline unsigned int read_uint(void) {
    char buf[24];
    read_line(buf, sizeof(buf));
    unsigned int result = 0;
    for (int i = 0; buf[i] >= '0' && buf[i] <= '9'; i++) {
        result = result * 10u + (unsigned int)(buf[i] - '0');
    }
    return result;
}

static inline void print_bin(unsigned int n) {
    for (int i = 31; i >= 0; i--) {
        print_char('0' + (char)((n >> i) & 1));
        if (i > 0 && i % 8 == 0) print_char(' ');
    }
}

static inline void eprint_char(char c) { __sys_write(STDERR, &c, 1); }
static inline void eprint_str(const char *s) { __sys_write(STDERR, s, (int)strlen(s)); }
static inline void eprint_ln(void) { eprint_char('\n'); }

static inline void eprint_uint(unsigned int n) {
    char buf[12];
    int i = 11;
    buf[i] = '\0';
    if (n == 0) { eprint_char('0'); return; }
    while (n > 0) { buf[--i] = '0' + (char)(n % 10); n /= 10; }
    eprint_str(buf + i);
}

static inline void eprint_int(int n) {
    if (n < 0) { eprint_char('-'); eprint_uint((unsigned int)(-n)); }
    else { eprint_uint((unsigned int)n); }
}

__attribute__((noreturn))
static inline void raven_panic(const char *msg) {
    __sys_write(STDERR, "PANIC: ", 7);
    __sys_write(STDERR, msg, (int)strlen(msg));
    __sys_write(STDERR, "\n", 1);
    raven_pause();
    __sys_exit(1);
}

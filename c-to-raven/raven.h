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

// ─────────────────────────────────────────────────────────────────────────────
// SYSCALL WRAPPERS
// ─────────────────────────────────────────────────────────────────────────────

static inline int sys_write(int fd, const void *buf, int len) {
    register int         _a7 __asm__("a7") = SYS_WRITE;
    register int         _a0 __asm__("a0") = fd;
    register const void *_a1 __asm__("a1") = buf;
    register int         _a2 __asm__("a2") = len;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2));
    return _a0;
}

static inline int sys_read(int fd, void *buf, int len) {
    register int   _a7 __asm__("a7") = SYS_READ;
    register int   _a0 __asm__("a0") = fd;
    register void *_a1 __asm__("a1") = buf;
    register int   _a2 __asm__("a2") = len;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2));
    return _a0;
}

__attribute__((noreturn))
static inline void sys_exit(int code) {
    register int _a7 __asm__("a7") = SYS_EXIT;
    register int _a0 __asm__("a0") = code;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
    __builtin_unreachable();
}

// exit_group(code) — syscall 94 (terminates all threads; behaves identically to
// sys_exit in Raven since it is single-threaded, but matches the Linux ABI).
__attribute__((noreturn))
static inline void sys_exit_group(int code) {
    register int _a7 __asm__("a7") = 94;
    register int _a0 __asm__("a0") = code;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
    __builtin_unreachable();
}

static inline int sys_getrandom(void *buf, int len, unsigned int flags) {
    register int          _a7 __asm__("a7") = SYS_GETRANDOM;
    register void        *_a0 __asm__("a0") = buf;
    register int          _a1 __asm__("a1") = len;
    register unsigned int _a2 __asm__("a2") = flags;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2));
    return ret;
}

// brk(addr) — syscall 214
// Pass 0 to query the current program break; pass a higher address to advance it.
// Returns the new (or current) break. Returns current break on failure.
static inline void *sys_brk(void *addr) {
    register int   _a7 __asm__("a7") = SYS_BRK;
    register void *_a0 __asm__("a0") = addr;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7));
    return _a0;
}

// getpid() — syscall 172 (always returns 1 in Raven)
static inline int sys_getpid(void) {
    register int _a7 __asm__("a7") = SYS_GETPID;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7));
    return ret;
}

// getuid() — syscall 174 (always returns 0 in Raven)
static inline int sys_getuid(void) {
    register int _a7 __asm__("a7") = SYS_GETUID;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7));
    return ret;
}

// getgid() — syscall 176 (always returns 0 in Raven)
static inline int sys_getgid(void) {
    register int _a7 __asm__("a7") = SYS_GETGID;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7));
    return ret;
}

// iovec struct for writev
typedef struct {
    void        *iov_base;  // buffer address
    unsigned int iov_len;   // buffer length in bytes
} raven_iovec;

// writev(fd, iov, iovcnt) — syscall 66
// Writes data from multiple buffers to fd.  Only fd=1 (stdout) and fd=2 (stderr).
static inline int sys_writev(int fd, const raven_iovec *iov, int iovcnt) {
    register int                  _a7 __asm__("a7") = SYS_WRITEV;
    register int                  _a0 __asm__("a0") = fd;
    register const raven_iovec   *_a1 __asm__("a1") = iov;
    register int                  _a2 __asm__("a2") = iovcnt;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2));
    return _a0;
}

// mmap(addr, len, prot, flags, fd, offset) — syscall 222
// Only anonymous mappings are supported (flags must include MAP_ANONYMOUS, fd must be -1).
// Allocates from the heap region.  Returns pointer on success, negative value on failure.
static inline void *sys_mmap(void *addr, size_t len, int prot, int flags, int fd, int offset) {
    register int   _a7 __asm__("a7") = SYS_MMAP;
    register void *_a0 __asm__("a0") = addr;
    register int   _a1 __asm__("a1") = (int)len;
    register int   _a2 __asm__("a2") = prot;
    register int   _a3 __asm__("a3") = flags;
    register int   _a4 __asm__("a4") = fd;
    register int   _a5 __asm__("a5") = offset;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2), "r"(_a3), "r"(_a4), "r"(_a5));
    return _a0;
}

// munmap(addr, len) — syscall 215
// No-op in Raven; always returns 0 (memory is never freed).
static inline int sys_munmap(void *addr, size_t len) {
    register int   _a7 __asm__("a7") = SYS_MUNMAP;
    register void *_a0 __asm__("a0") = addr;
    register int   _a1 __asm__("a1") = (int)len;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1));
    return _a0;
}

// timespec for clock_gettime
typedef struct {
    unsigned int tv_sec;   // seconds
    unsigned int tv_nsec;  // nanoseconds
} raven_timespec;

// clock_gettime(clockid, tp) — syscall 403
// Fills *tp with simulated time derived from instruction count (~10 ns per instruction).
static inline int sys_clock_gettime(int clockid, raven_timespec *tp) {
    register int              _a7 __asm__("a7") = SYS_CLOCK_GETTIME;
    register int              _a0 __asm__("a0") = clockid;
    register raven_timespec  *_a1 __asm__("a1") = tp;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1));
    return _a0;
}

// ─────────────────────────────────────────────────────────────────────────────
// SIMULATOR CONTROL
// ─────────────────────────────────────────────────────────────────────────────

// Pause execution so you can inspect registers / memory in Raven.
static inline void raven_pause(void) {
    __asm__ volatile("ebreak");
}

// ─────────────────────────────────────────────────────────────────────────────
// MEMORY UTILITIES
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// STRING UTILITIES
// ─────────────────────────────────────────────────────────────────────────────

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

// Returns pointer to first occurrence of c in s, or NULL.
static inline char *strchr(const char *s, int c) {
    while (*s) {
        if (*s == (char)c) return (char *)s;
        s++;
    }
    return (c == '\0') ? (char *)s : NULL;
}

// Returns pointer to last occurrence of c in s, or NULL.
static inline char *strrchr(const char *s, int c) {
    const char *last = NULL;
    do {
        if (*s == (char)c) last = s;
    } while (*s++);
    return (char *)last;
}

// ─────────────────────────────────────────────────────────────────────────────
// RANDOM UTILITIES  (backed by getrandom — cryptographic quality)
// ─────────────────────────────────────────────────────────────────────────────

// Return a uniformly random 32-bit unsigned integer.
static inline unsigned int rand_u32(void) {
    unsigned int v;
    sys_getrandom(&v, (int)sizeof(v), 0);
    return v;
}

// Return a uniformly random byte (0–255).
static inline unsigned char rand_u8(void) {
    unsigned char v;
    sys_getrandom(&v, 1, 0);
    return v;
}

// Return a random unsigned int in [lo, hi).  Returns lo if hi <= lo.
// Note: uses modulo reduction — fine for teaching, not cryptographic use.
static inline unsigned int rand_range(unsigned int lo, unsigned int hi) {
    if (hi <= lo) return lo;
    return lo + rand_u32() % (hi - lo);
}

// Return a random int in [-2147483648, 2147483647].
static inline int rand_i32(void) {
    return (int)rand_u32();
}

// Return 0 or 1 with equal probability.
static inline int rand_bool(void) {
    return (int)(rand_u8() & 1u);
}

// ─────────────────────────────────────────────────────────────────────────────
// MATH UTILITIES
// ─────────────────────────────────────────────────────────────────────────────

static inline int abs(int n)          { return n < 0 ? -n : n; }
static inline int min(int a, int b)   { return a < b ? a : b; }
static inline int max(int a, int b)   { return a > b ? a : b; }

static inline unsigned int umin(unsigned int a, unsigned int b) { return a < b ? a : b; }
static inline unsigned int umax(unsigned int a, unsigned int b) { return a > b ? a : b; }

// Integer power: base^exp (no overflow check).
static inline int ipow(int base, unsigned int exp) {
    int result = 1;
    while (exp--) result *= base;
    return result;
}

// ─────────────────────────────────────────────────────────────────────────────
// I/O HELPERS
// ─────────────────────────────────────────────────────────────────────────────

static inline void print_char(char c) {
    sys_write(STDOUT, &c, 1);
}

static inline void print_str(const char *s) {
    sys_write(STDOUT, s, (int)strlen(s));
}

static inline void print_ln(void) { print_char('\n'); }

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
    else        { print_uint((unsigned int)n); }
}

// Print unsigned int as hex with "0x" prefix, zero-padded to 8 digits.
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

// Print a pointer as hex address.
static inline void print_ptr(const void *p) {
    print_hex((unsigned int)(size_t)p);
}

// Print a float with `decimals` decimal places (0–6).
static inline void print_float(float v, int decimals) {
    if (v < 0.0f) { print_char('-'); v = -v; }
    unsigned int i = (unsigned int)v;
    print_uint(i);
    if (decimals > 0) {
        print_char('.');
        // isolate fractional part
        float frac = v - (float)i;
        while (decimals--) {
            frac *= 10.0f;
            int d = (int)frac;
            print_char('0' + (char)d);
            frac -= (float)d;
        }
    }
}

// Print boolean as "true" / "false".
static inline void print_bool(int v) {
    print_str(v ? "true" : "false");
}

// Read a single byte from stdin. Returns the character as unsigned char cast
// to int, or -1 on EOF / error (same convention as C's getchar).
static inline int read_char(void) {
    unsigned char c;
    int n = sys_read(STDIN, &c, 1);
    return n > 0 ? (int)c : -1;
}

// Read a line from stdin. Stops on newline or EOF. Always null-terminates.
// Returns number of bytes read (not counting the '\0').
static inline int read_line(char *buf, int max) {
    int n = 0;
    while (n < max - 1) {
        char c;
        if (sys_read(STDIN, &c, 1) <= 0 || c == '\n') break;
        buf[n++] = c;
    }
    buf[n] = '\0';
    return n;
}

// Read an integer from stdin (decimal, optional leading '-').
static inline int read_int(void) {
    char buf[24];
    read_line(buf, sizeof(buf));
    int sign = 1, i = 0, result = 0;
    if (buf[i] == '-') { sign = -1; i++; }
    for (; buf[i] >= '0' && buf[i] <= '9'; i++)
        result = result * 10 + (buf[i] - '0');
    return sign * result;
}

// Read an unsigned decimal integer from stdin.
static inline unsigned int read_uint(void) {
    char buf[24];
    read_line(buf, sizeof(buf));
    unsigned int result = 0;
    for (int i = 0; buf[i] >= '0' && buf[i] <= '9'; i++)
        result = result * 10u + (unsigned int)(buf[i] - '0');
    return result;
}

// Print unsigned int as 32-bit binary, grouped by byte: "10110011 00101010 ..."
static inline void print_bin(unsigned int n) {
    for (int i = 31; i >= 0; i--) {
        print_char('0' + (char)((n >> i) & 1));
        if (i > 0 && i % 8 == 0) print_char(' ');
    }
}

// Stderr convenience helpers for debugging.
static inline void eprint_char(char c)       { sys_write(STDERR, &c, 1); }
static inline void eprint_str(const char *s) { sys_write(STDERR, s, (int)strlen(s)); }
static inline void eprint_ln(void)           { eprint_char('\n'); }
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
    else        { eprint_uint((unsigned int)n); }
}

// ─────────────────────────────────────────────────────────────────────────────
// ASSERT / PANIC
// ─────────────────────────────────────────────────────────────────────────────

// Print a message to stderr and exit with code 1.
__attribute__((noreturn))
static inline void raven_panic(const char *msg) {
    sys_write(STDERR, "PANIC: ", 7);
    sys_write(STDERR, msg, (int)strlen(msg));
    sys_write(STDERR, "\n", 1);
    raven_pause(); // freeze so you can inspect state in Raven before exit
    sys_exit(1);
}

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

#ifndef RAVEN_HEAP_SIZE
#define RAVEN_HEAP_SIZE (64 * 1024)  // 64 KB default
#endif

// Block header stored immediately before each allocation.
typedef struct _rh_block {
    unsigned int      size;  // usable bytes (not counting this header)
    unsigned int      free;  // 1 = available, 0 = in use
    struct _rh_block *next;  // next block in the list (NULL = last)
} _rh_block_t;

static char      _raven_heap[RAVEN_HEAP_SIZE];
static _rh_block_t *_raven_heap_head = NULL;

static inline void _raven_heap_init(void) {
    _raven_heap_head = (_rh_block_t *)_raven_heap;
    _raven_heap_head->size = RAVEN_HEAP_SIZE - (unsigned int)sizeof(_rh_block_t);
    _raven_heap_head->free = 1;
    _raven_heap_head->next = NULL;
}

// Allocate `size` bytes. Returns NULL on out-of-memory.
static inline void *malloc(size_t size) {
    if (!_raven_heap_head) _raven_heap_init();
    if (size == 0) return NULL;

    // Align to 4 bytes
    size = (size + 3u) & ~3u;

    _rh_block_t *b = _raven_heap_head;
    while (b) {
        if (b->free && b->size >= size) {
            // Split the block if there is room for another header + at least 4 bytes
            if (b->size >= size + (unsigned int)sizeof(_rh_block_t) + 4u) {
                _rh_block_t *next = (_rh_block_t *)((char *)(b + 1) + size);
                next->size = b->size - size - (unsigned int)sizeof(_rh_block_t);
                next->free = 1;
                next->next = b->next;
                b->next    = next;
                b->size    = size;
            }
            b->free = 0;
            return (void *)(b + 1);
        }
        b = b->next;
    }
    return NULL; // out of memory
}

// Allocate `nmemb * size` bytes, zero-initialised.
static inline void *calloc(size_t nmemb, size_t size) {
    size_t total = nmemb * size;
    void *p = malloc(total);
    if (p) memset(p, 0, total);
    return p;
}

// Resize a previous allocation. If `ptr` is NULL, behaves like malloc.
static inline void *realloc(void *ptr, size_t new_size) {
    if (!ptr)       return malloc(new_size);
    if (!new_size)  { /* free(ptr); */ return NULL; }

    _rh_block_t *b = (_rh_block_t *)ptr - 1;
    if (b->size >= new_size) return ptr; // block is already large enough

    void *fresh = malloc(new_size);
    if (!fresh) return NULL;
    memcpy(fresh, ptr, b->size);

    // free old block
    b->free = 1;
    // coalesce with next free blocks
    while (b->next && b->next->free) {
        b->size += sizeof(_rh_block_t) + b->next->size;
        b->next  = b->next->next;
    }
    return fresh;
}

// Free a previously malloc'd pointer. Coalesces adjacent free blocks.
static inline void free(void *ptr) {
    if (!ptr) return;
    _rh_block_t *b = (_rh_block_t *)ptr - 1;
    b->free = 1;
    // Coalesce with subsequent free neighbours
    while (b->next && b->next->free) {
        b->size += (unsigned int)sizeof(_rh_block_t) + b->next->size;
        b->next  = b->next->next;
    }
}

// Return total bytes still available in the heap (approximate).
static inline size_t raven_heap_free(void) {
    if (!_raven_heap_head) return RAVEN_HEAP_SIZE - sizeof(_rh_block_t);
    size_t total = 0;
    _rh_block_t *b = _raven_heap_head;
    while (b) {
        if (b->free) total += b->size;
        b = b->next;
    }
    return total;
}

// Return total bytes currently in use on the heap.
static inline size_t raven_heap_used(void) {
    return RAVEN_HEAP_SIZE - sizeof(_rh_block_t) - raven_heap_free();
}

// ─────────────────────────────────────────────────────────────────────────────
// FALCON TEACHING EXTENSIONS  (syscalls 1000–1012)
//
// These are Raven-specific shortcuts — simpler than the Linux ABI wrappers
// above because they need no strlen loop or fd argument.  Useful in very
// small programs where you want the minimal call sequence.
// ─────────────────────────────────────────────────────────────────────────────

// Print signed 32-bit integer to console (no newline).  — syscall 1000
static inline void falcon_print_int(int n) {
    register int _a7 __asm__("a7") = 1000;
    register int _a0 __asm__("a0") = n;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// Print NUL-terminated string (no newline).  — syscall 1001
static inline void falcon_print_str(const char *s) {
    register int          _a7 __asm__("a7") = 1001;
    register const char  *_a0 __asm__("a0") = s;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// Print NUL-terminated string followed by newline.  — syscall 1002
static inline void falcon_println_str(const char *s) {
    register int          _a7 __asm__("a7") = 1002;
    register const char  *_a0 __asm__("a0") = s;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// Read one line from console into buf (NUL-terminated, newline excluded).
// Caller must ensure buf is large enough.  — syscall 1003
static inline void falcon_read_line(char *buf) {
    register int   _a7 __asm__("a7") = 1003;
    register char *_a0 __asm__("a0") = buf;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// Read one unsigned byte from stdin and store it at *dst.  — syscall 1010
// Accepts decimal or 0x-prefixed hex; range 0..255.
static inline void falcon_read_u8(unsigned char *dst) {
    register int            _a7 __asm__("a7") = 1010;
    register unsigned char *_a0 __asm__("a0") = dst;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// Read one unsigned 16-bit integer from stdin and store it at *dst.  — syscall 1011
static inline void falcon_read_u16(unsigned short *dst) {
    register int             _a7 __asm__("a7") = 1011;
    register unsigned short *_a0 __asm__("a0") = dst;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// Read one unsigned 32-bit integer from stdin and store it at *dst.  — syscall 1012
static inline void falcon_read_u32(unsigned int *dst) {
    register int           _a7 __asm__("a7") = 1012;
    register unsigned int *_a0 __asm__("a0") = dst;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// Print unsigned 32-bit integer to console (no newline).  — syscall 1004
static inline void falcon_print_uint(unsigned int n) {
    register int          _a7 __asm__("a7") = 1004;
    register unsigned int _a0 __asm__("a0") = n;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// Print value as hex with "0x" prefix (e.g. "0xDEADBEEF"), no newline.  — syscall 1005
static inline void falcon_print_hex(unsigned int n) {
    register int          _a7 __asm__("a7") = 1005;
    register unsigned int _a0 __asm__("a0") = n;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// Print a single ASCII character.  — syscall 1006
static inline void falcon_print_char(char c) {
    register int _a7 __asm__("a7") = 1006;
    register int _a0 __asm__("a0") = (unsigned char)c;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// Print a newline character.  — syscall 1008
static inline void falcon_print_newline(void) {
    register int _a7 __asm__("a7") = 1008;
    __asm__ volatile("ecall" :: "r"(_a7));
}

// Read one signed 32-bit integer from stdin (accepts negatives).  — syscall 1013
static inline void falcon_read_int(int *dst) {
    register int  _a7 __asm__("a7") = 1013;
    register int *_a0 __asm__("a0") = dst;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// Read one IEEE 754 float from stdin.  — syscall 1014
static inline void falcon_read_float(float *dst) {
    register int    _a7 __asm__("a7") = 1014;
    register float *_a0 __asm__("a0") = dst;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// Print float value in fa0 to console (up to 6 significant digits, no newline).  — syscall 1015
// Note: passes the float value via the fa0 floating-point register.
static inline void falcon_print_float(float v) {
    register int _a7 __asm__("a7") = 1015;
    __asm__ volatile("ecall" :: "r"(_a7), "f"(v));
}

// Return the number of instructions executed since program start (low 32 bits).  — syscall 1030
// Useful for measuring the cost of algorithm sections without leaving the simulator.
static inline unsigned int falcon_get_instr_count(void) {
    register int          _a7 __asm__("a7") = 1030;
    register unsigned int _a0;
    __asm__ volatile("ecall" : "=r"(_a0) : "r"(_a7));
    return _a0;
}

// Alias of falcon_get_instr_count.  — syscall 1031
static inline unsigned int falcon_get_cycle_count(void) {
    register int          _a7 __asm__("a7") = 1031;
    register unsigned int _a0;
    __asm__ volatile("ecall" : "=r"(_a0) : "r"(_a7));
    return _a0;
}

// ─────────────────────────────────────────────────────────────────────────────
// FALCON MEMORY UTILITIES  (syscalls 1050–1053)
//
// Simulator-side versions of memset/memcpy/strlen/strcmp.
// These call into the simulator directly instead of running a C loop.
// Useful for benchmarking: compare falcon_get_instr_count() before/after.
// Prefixed with falcon_ to coexist with the C implementations above.
// ─────────────────────────────────────────────────────────────────────────────

// Fill `len` bytes at `dst` with `byte` (via syscall 1050).
static inline void falcon_memset(void *dst, unsigned char byte, size_t len) {
    register int          _a7 __asm__("a7") = 1050;
    register void        *_a0 __asm__("a0") = dst;
    register unsigned int _a1 __asm__("a1") = (unsigned int)byte;
    register int          _a2 __asm__("a2") = (int)len;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2));
}

// Copy `len` bytes from `src` to `dst` (via syscall 1051).  Regions must not overlap.
static inline void falcon_memcpy(void *dst, const void *src, size_t len) {
    register int          _a7 __asm__("a7") = 1051;
    register void        *_a0 __asm__("a0") = dst;
    register const void  *_a1 __asm__("a1") = src;
    register int          _a2 __asm__("a2") = (int)len;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2));
}

// Return the length of NUL-terminated string at `s` (via syscall 1052).
static inline size_t falcon_strlen(const char *s) {
    register int          _a7 __asm__("a7") = 1052;
    register const char  *_a0 __asm__("a0") = s;
    unsigned int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0));
    return (size_t)ret;
}

// Compare NUL-terminated strings `s1` and `s2` (via syscall 1053).
// Returns negative / 0 / positive (same as C strcmp).
static inline int falcon_strcmp(const char *s1, const char *s2) {
    register int         _a7 __asm__("a7") = 1053;
    register const char *_a0 __asm__("a0") = s1;
    register const char *_a1 __asm__("a1") = s2;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0), "r"(_a1));
    return ret;
}

// ─────────────────────────────────────────────────────────────────────────────
// HART MANAGEMENT  (syscall 1100)
//
// Spawns a new hardware thread (hart) to run concurrently with the caller.
// The new hart begins execution at `entry_pc` with stack pointer `stack_ptr`.
// `arg` is placed in a0 so the hart entry point receives a single u32 argument.
//
// Returns 0 on success.  In Raven, the new hart is scheduled alongside the
// caller from the next simulation cycle.
//
// Typical usage:
//
//   void worker(unsigned int id) { ... sys_exit(0); }
//
//   // allocate a stack for the new hart
//   static char hart1_stack[4096];
//   falcon_hart_start((unsigned int)worker,
//                     (unsigned int)(hart1_stack + sizeof(hart1_stack)),
//                     /*arg=*/1);
// ─────────────────────────────────────────────────────────────────────────────

// Convenience macro — spawns a hart using a stack array declared in scope.
// stack_arr must be a char/u8 array (not a pointer).  Computes stack-top automatically.
//
//   static char worker_stack[4096];
//   RAVEN_SPAWN_HART(my_worker, worker_stack, /*arg=*/1);
#define RAVEN_SPAWN_HART(fn_ptr, stack_arr, arg) \
    falcon_hart_start((unsigned int)(fn_ptr), \
                      (unsigned int)((stack_arr) + sizeof(stack_arr)), \
                      (unsigned int)(arg))

// Spawn a new hart.  entry_pc must point to a valid instruction.
// stack_ptr must point to the TOP (high address) of an aligned stack region.
// arg is passed in a0 of the new hart.  Returns 0.
static inline int falcon_hart_start(unsigned int entry_pc,
                                    unsigned int stack_ptr,
                                    unsigned int arg) {
    register int          _a7 __asm__("a7") = 1100;
    register unsigned int _a0 __asm__("a0") = entry_pc;
    register unsigned int _a1 __asm__("a1") = stack_ptr;
    register unsigned int _a2 __asm__("a2") = arg;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2));
    return (int)_a0;
}

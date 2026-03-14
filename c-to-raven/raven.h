#pragma once

// ─────────────────────────────────────────────────────────────────────────────
// raven.h  —  bare-metal runtime for RISC-V programs running in Raven
//
// No libc, no OS. Everything here is self-contained static inline.
// Include once in your .c file and you get syscalls, I/O, strings,
// memory utilities, a heap allocator, and simulator control.
// ─────────────────────────────────────────────────────────────────────────────

// ── Syscall numbers ──────────────────────────────────────────────────────────
#define SYS_READ      63
#define SYS_WRITE     64
#define SYS_EXIT      93
#define SYS_GETRANDOM 278

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

static inline int sys_getrandom(void *buf, int len, unsigned int flags) {
    register int          _a7 __asm__("a7") = SYS_GETRANDOM;
    register void        *_a0 __asm__("a0") = buf;
    register int          _a1 __asm__("a1") = len;
    register unsigned int _a2 __asm__("a2") = flags;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2));
    return ret;
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

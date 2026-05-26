#ifndef RAVEN_MEM_H
#define RAVEN_MEM_H

#include <raven/types.h>
#include <raven/_ecall.h>

/* ── Heap (first-fit allocator, defined in libraven.a) ─────────────────────
 *
 * The heap is a static buffer of RAVEN_HEAP_SIZE bytes (default 64 KB).
 * To resize, rebuild libraven.a with -DRAVEN_HEAP_SIZE=<bytes>. */

void        *raven_malloc (raven_size_t size);
void        *raven_calloc (raven_size_t nmemb, raven_size_t size);
void        *raven_realloc(void *ptr, raven_size_t new_size);
void         raven_free   (void *ptr);
raven_size_t raven_heap_used(void);
raven_size_t raven_heap_free(void);

/* ── memcpy / memset family ───────────────────────────────────────────────
 *
 * Two flavors per primitive:
 *   raven_X    — simulator-accelerated; one ecall, work invisible to instr_count
 *   raven_X_c  — pure C loop; cost counts toward instr_count
 *
 * The plain raven_X form is the default; use raven_X_c when measuring. */

static inline void raven_memset(void *dst, raven_u8 byte, raven_size_t len) {
    register int          _a7 __asm__("a7") = RAVEN_ECALL_MEMSET;
    register void        *_a0 __asm__("a0") = dst;
    register raven_u32    _a1 __asm__("a1") = (raven_u32)byte;
    register int          _a2 __asm__("a2") = (int)len;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2) : "memory");
}

static inline void raven_memcpy(void *dst, const void *src, raven_size_t len) {
    register int          _a7 __asm__("a7") = RAVEN_ECALL_MEMCPY;
    register void        *_a0 __asm__("a0") = dst;
    register const void  *_a1 __asm__("a1") = src;
    register int          _a2 __asm__("a2") = (int)len;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2) : "memory");
}

void *raven_memset_c(void *dst, raven_u8 byte, raven_size_t len);
void *raven_memcpy_c(void *dst, const void *src, raven_size_t len);
void *raven_memmove (void *dst, const void *src, raven_size_t len);
int   raven_memcmp  (const void *a, const void *b, raven_size_t n);

#endif

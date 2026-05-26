#include <raven/mem.h>
#include "internal/heap_block.h"

/* ── Heap state (one copy, linked once) ──────────────────────────────────── */

char             _raven_heap[RAVEN_HEAP_SIZE];
_RavenHeapBlock *_raven_heap_head = (_RavenHeapBlock *)0;

void _raven_heap_init(void) {
    _raven_heap_head = (_RavenHeapBlock *)_raven_heap;
    _raven_heap_head->size = RAVEN_HEAP_SIZE - (raven_u32)sizeof(_RavenHeapBlock);
    _raven_heap_head->free = 1;
    _raven_heap_head->next = (_RavenHeapBlock *)0;
}

/* ── malloc family (first-fit) ───────────────────────────────────────────── */

void *raven_malloc(raven_size_t size) {
    if (size == 0) return (void *)0;
    if (!_raven_heap_head) _raven_heap_init();

    /* 4-byte alignment of payload */
    size = (size + 3u) & ~3u;

    for (_RavenHeapBlock *b = _raven_heap_head; b; b = b->next) {
        if (!b->free || b->size < size) continue;

        /* split if there's room for a useful trailer block */
        if (b->size > size + sizeof(_RavenHeapBlock) + 4u) {
            _RavenHeapBlock *n = (_RavenHeapBlock *)((char *)b + sizeof(_RavenHeapBlock) + size);
            n->size = b->size - size - (raven_u32)sizeof(_RavenHeapBlock);
            n->free = 1;
            n->next = b->next;
            b->next = n;
            b->size = size;
        }
        b->free = 0;
        return (char *)b + sizeof(_RavenHeapBlock);
    }
    return (void *)0;   /* out of memory */
}

void raven_free(void *ptr) {
    if (!ptr) return;
    _RavenHeapBlock *b = (_RavenHeapBlock *)((char *)ptr - sizeof(_RavenHeapBlock));
    b->free = 1;

    /* coalesce free neighbours */
    for (_RavenHeapBlock *c = _raven_heap_head; c && c->next; ) {
        if (c->free && c->next->free) {
            c->size += (raven_u32)sizeof(_RavenHeapBlock) + c->next->size;
            c->next = c->next->next;
        } else {
            c = c->next;
        }
    }
}

void *raven_calloc(raven_size_t nmemb, raven_size_t size) {
    raven_size_t total = nmemb * size;
    void *p = raven_malloc(total);
    if (p) raven_memset_c(p, 0, total);
    return p;
}

void *raven_realloc(void *ptr, raven_size_t new_size) {
    if (!ptr)            return raven_malloc(new_size);
    if (new_size == 0)   { raven_free(ptr); return (void *)0; }

    _RavenHeapBlock *b = (_RavenHeapBlock *)((char *)ptr - sizeof(_RavenHeapBlock));
    if (b->size >= new_size) return ptr;   /* shrink-or-equal in place */

    void *new_ptr = raven_malloc(new_size);
    if (!new_ptr) return (void *)0;
    raven_memcpy_c(new_ptr, ptr, b->size);
    raven_free(ptr);
    return new_ptr;
}

raven_size_t raven_heap_used(void) {
    if (!_raven_heap_head) return 0;
    raven_size_t used = 0;
    for (_RavenHeapBlock *b = _raven_heap_head; b; b = b->next)
        if (!b->free) used += b->size + (raven_u32)sizeof(_RavenHeapBlock);
    return used;
}

raven_size_t raven_heap_free(void) {
    if (!_raven_heap_head) return RAVEN_HEAP_SIZE;
    raven_size_t fre = 0;
    for (_RavenHeapBlock *b = _raven_heap_head; b; b = b->next)
        if (b->free) fre += b->size;
    return fre;
}

/* ── memcpy / memset C-loop variants (cost counts toward instr_count) ────── */

void *raven_memset_c(void *dst, raven_u8 byte, raven_size_t len) {
    raven_u8 *p = (raven_u8 *)dst;
    while (len--) *p++ = byte;
    return dst;
}

void *raven_memcpy_c(void *dst, const void *src, raven_size_t len) {
    raven_u8       *d = (raven_u8       *)dst;
    const raven_u8 *s = (const raven_u8 *)src;
    while (len--) *d++ = *s++;
    return dst;
}

void *raven_memmove(void *dst, const void *src, raven_size_t len) {
    raven_u8       *d = (raven_u8       *)dst;
    const raven_u8 *s = (const raven_u8 *)src;
    if (d < s) {
        while (len--) *d++ = *s++;
    } else if (d > s) {
        d += len; s += len;
        while (len--) *--d = *--s;
    }
    return dst;
}

int raven_memcmp(const void *a, const void *b, raven_size_t n) {
    const raven_u8 *p = (const raven_u8 *)a;
    const raven_u8 *q = (const raven_u8 *)b;
    while (n--) {
        if (*p != *q) return (int)*p - (int)*q;
        p++; q++;
    }
    return 0;
}

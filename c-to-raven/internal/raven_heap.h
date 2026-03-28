#pragma once

// Internal heap allocator layer for raven.h.

#ifndef RAVEN_HEAP_SIZE
#define RAVEN_HEAP_SIZE (64 * 1024)
#endif

#include "raven_internal.h"

static inline void *malloc(size_t size) {
    if (!__raven_heap_head) __raven_heap_init();
    if (size == 0) return NULL;

    size = (size + 3u) & ~3u;

    __rh_block_t *b = __raven_heap_head;
    while (b) {
        if (b->free && b->size >= size) {
            if (b->size >= size + (unsigned int)sizeof(__rh_block_t) + 4u) {
                __rh_block_t *next = (__rh_block_t *)((char *)(b + 1) + size);
                next->size = b->size - size - (unsigned int)sizeof(__rh_block_t);
                next->free = 1;
                next->next = b->next;
                b->next = next;
                b->size = size;
            }
            b->free = 0;
            return (void *)(b + 1);
        }
        b = b->next;
    }
    return NULL;
}

static inline void *calloc(size_t nmemb, size_t size) {
    size_t total = nmemb * size;
    void *p = malloc(total);
    if (p) memset(p, 0, total);
    return p;
}

static inline void *realloc(void *ptr, size_t new_size) {
    if (!ptr) return malloc(new_size);
    if (!new_size) return NULL;

    __rh_block_t *b = (__rh_block_t *)ptr - 1;
    if (b->size >= new_size) return ptr;

    void *fresh = malloc(new_size);
    if (!fresh) return NULL;
    memcpy(fresh, ptr, b->size);

    b->free = 1;
    while (b->next && b->next->free) {
        b->size += sizeof(__rh_block_t) + b->next->size;
        b->next = b->next->next;
    }
    return fresh;
}

static inline void free(void *ptr) {
    if (!ptr) return;
    __rh_block_t *b = (__rh_block_t *)ptr - 1;
    b->free = 1;
    while (b->next && b->next->free) {
        b->size += (unsigned int)sizeof(__rh_block_t) + b->next->size;
        b->next = b->next->next;
    }
}

static inline size_t raven_heap_free(void) {
    if (!__raven_heap_head) return RAVEN_HEAP_SIZE - sizeof(__rh_block_t);
    size_t total = 0;
    __rh_block_t *b = __raven_heap_head;
    while (b) {
        if (b->free) total += b->size;
        b = b->next;
    }
    return total;
}

static inline size_t raven_heap_used(void) {
    return RAVEN_HEAP_SIZE - sizeof(__rh_block_t) - raven_heap_free();
}

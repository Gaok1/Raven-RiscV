#pragma once

// Internal implementation details for raven.h.
// This header is not intended for direct user inclusion.

typedef struct __rh_block {
    unsigned int       size;  // usable bytes (not counting this header)
    unsigned int       free;  // 1 = available, 0 = in use
    struct __rh_block *next;  // next block in the list (NULL = last)
} __rh_block_t;

static char         __raven_heap[RAVEN_HEAP_SIZE];
static __rh_block_t *__raven_heap_head = NULL;

static inline void __raven_heap_init(void) {
    __raven_heap_head = (__rh_block_t *)__raven_heap;
    __raven_heap_head->size = RAVEN_HEAP_SIZE - (unsigned int)sizeof(__rh_block_t);
    __raven_heap_head->free = 1;
    __raven_heap_head->next = NULL;
}

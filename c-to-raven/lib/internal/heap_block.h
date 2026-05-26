#ifndef RAVEN_LIB_INTERNAL_HEAP_BLOCK_H
#define RAVEN_LIB_INTERNAL_HEAP_BLOCK_H

/* Heap allocator internals. NOT on the public include path.
 * Translation units under lib/ can reach this; user code cannot. */

#include <raven/types.h>

#ifndef RAVEN_HEAP_SIZE
#define RAVEN_HEAP_SIZE (64 * 1024)
#endif

typedef struct _RavenHeapBlock {
    raven_u32                size;   /* usable bytes (excluding this header) */
    raven_u32                free;   /* 1 = available, 0 = in use            */
    struct _RavenHeapBlock  *next;
} _RavenHeapBlock;

extern char             _raven_heap[RAVEN_HEAP_SIZE];
extern _RavenHeapBlock *_raven_heap_head;

void _raven_heap_init(void);

#endif

#ifndef RAVEN_LIB_INTERNAL_HART_PAYLOAD_H
#define RAVEN_LIB_INTERNAL_HART_PAYLOAD_H

/* Hart spawn machinery internals. NOT on the public include path. */

#include <raven/hart.h>

typedef struct _RavenHartPayload {
    RavenHartEntry entry;
    unsigned int   arg;
    volatile int   done;
    int            self_free;   /* if 1, trampoline frees payload on exit */
} _RavenHartPayload;

__attribute__((noreturn))
void _raven_hart_trampoline(unsigned int payload_ptr);

#endif

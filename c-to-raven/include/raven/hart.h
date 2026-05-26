#ifndef RAVEN_HART_H
#define RAVEN_HART_H

#include <raven/types.h>

/* Hart entry function signature. The single unsigned int argument carries
 * whatever value you passed at spawn time. */
typedef void (*RavenHartEntry)(unsigned int arg);

/* Opaque-ish handle. The struct is defined here only so callers can place
 * it on the stack; do NOT read or write the _payload field directly. */
typedef struct RavenHart {
    void *_payload;
} RavenHart;

/* Task descriptor — a hart's launch parameters bundled into one value. */
typedef struct RavenHartTask {
    RavenHartEntry entry;
    void          *stack_base;
    raven_size_t   stack_size;
    unsigned int   arg;
} RavenHartTask;

/* Declare a 16-byte-aligned stack buffer for a hart. The RISC-V ABI requires
 * sp to be 16-byte aligned at function entry.
 *
 *   RAVEN_HART_STACK(worker_stack, 4096);
 *   RavenHart h = RAVEN_HART_SPAWN(worker, worker_stack, 42);
 */
#define RAVEN_HART_STACK(name, size) \
    static char name[(size)] __attribute__((aligned(16)))

/* Internal: reject non-array values in the *_array macros so a malloc'd
 * pointer doesn't silently pass sizeof(void*) as the stack size. */
#define _RAVEN_REQUIRE_ARRAY(value)                                            \
    ((void)sizeof(char[__builtin_types_compatible_p(__typeof__(value),          \
                                                    __typeof__(&(value)[0]))    \
                       ? -1 : 1]))

/* ── Spawning ────────────────────────────────────────────────────────────── */

RavenHart raven_hart_spawn(RavenHartEntry entry,
                           void          *stack_base,
                           raven_size_t   stack_size,
                           unsigned int   arg);

RavenHart raven_hart_start(const RavenHartTask *task);

static inline RavenHartTask raven_hart_task(RavenHartEntry entry,
                                            void         *stack_base,
                                            raven_size_t  stack_size,
                                            unsigned int  arg) {
    RavenHartTask t;
    t.entry      = entry;
    t.stack_base = stack_base;
    t.stack_size = stack_size;
    t.arg        = arg;
    return t;
}

/* Stack-array helpers: stack_size is computed automatically from the array. */
#define RAVEN_HART_SPAWN(fn, stack_array, arg)                                 \
    (_RAVEN_REQUIRE_ARRAY(stack_array),                                        \
     raven_hart_spawn((fn), (stack_array), sizeof(stack_array),                \
                      (unsigned int)(arg)))

#define RAVEN_HART_TASK(fn, stack_array, arg)                                  \
    (_RAVEN_REQUIRE_ARRAY(stack_array),                                        \
     raven_hart_task((fn), (stack_array), sizeof(stack_array),                 \
                     (unsigned int)(arg)))

/* ── Lifecycle ───────────────────────────────────────────────────────────── */

/* 1 if the hart has exited, 0 if still running. */
int  raven_hart_is_done(RavenHart h);

/* Spin-wait until the hart exits, then free its internal resources and
 * invalidate the handle. */
void raven_hart_join(RavenHart *h);

/* Abandon the hart: it will free its own resources when it exits. The
 * handle is invalidated immediately; do not call is_done or join afterward. */
void raven_hart_detach(RavenHart *h);

#endif

#ifndef RAVEN_CORO_H
#define RAVEN_CORO_H

#include <raven/types.h>

/* ─────────────────────────────────────────────────────────────────────────
 *  coro.h — stackful cooperative coroutines for Raven
 *
 *  A coroutine is a function that runs on its own stack and can suspend itself
 *  with raven_coro_yield(), handing control back to whoever resumed it. Its
 *  stack and registers stay alive across the suspension, so the next
 *  raven_coro_resume() continues exactly where it left off.
 *
 *  Coroutines are cooperative and single-hart: exactly one runs at a time and
 *  control only moves on an explicit resume/yield. This is NOT the same as
 *  <raven/hart.h>, which runs code in parallel on another hart. A coroutine
 *  switch is a pure user-space register/stack swap — no ecall is involved.
 *
 *  resume and yield exchange one value (a void*) in each direction, which is
 *  all you need for the generator pattern. Pass NULL and ignore the result if
 *  you don't care about the value.
 *
 *      void counter(RavenCoro *self, void *arg) {
 *          int n = (int)(raven_uintptr_t)arg;
 *          for (int i = 1; i <= n; i++)
 *              raven_coro_yield(self, (void *)(raven_uintptr_t)i);
 *      }
 *
 *      RAVEN_CORO_STACK(stack, 4096);
 *      RavenCoro co;
 *      raven_coro_init(&co, stack, sizeof(stack), counter,
 *                      (void *)(raven_uintptr_t)5);
 *      while (!raven_coro_done(&co)) {
 *          void *v = raven_coro_resume(&co, NULL);
 *          if (!raven_coro_done(&co)) { ... use v ... }
 *      }
 * ───────────────────────────────────────────────────────────────────────── */

typedef struct RavenCoro RavenCoro;

/* Coroutine body. `self` is the running coroutine (pass it to
 * raven_coro_yield); `arg` is the value handed to raven_coro_init. */
typedef void (*RavenCoroFn)(RavenCoro *self, void *arg);

typedef enum RavenCoroState {
    RAVEN_CORO_READY = 0,   /* initialized, not yet started        */
    RAVEN_CORO_SUSPENDED,   /* yielded; resumable                  */
    RAVEN_CORO_RUNNING,     /* currently executing                 */
    RAVEN_CORO_DONE,        /* body returned; no longer resumable  */
} RavenCoroState;

/* Saved callee-saved register block. Internal — do not read or write its
 * fields. Exposed only so RavenCoro has a known size. The field offsets are
 * hard-coded in lib/coro_switch.S and must stay in sync. */
typedef struct _RavenCoroCtx {
    raven_u32 ra;
    raven_u32 sp;
    raven_u32 s[12];        /* s0..s11  (x8, x9, x18..x27) */
#if defined(__riscv_flen)
    raven_u32 fs[12];       /* fs0..fs11 (f8, f9, f18..f27) — hard-float only */
#endif
} _RavenCoroCtx;

/* A coroutine. Place it on the stack or in static storage; treat the fields as
 * opaque (use the functions below). */
struct RavenCoro {
    _RavenCoroCtx  ctx;       /* coroutine's own saved context  */
    _RavenCoroCtx  caller;    /* resumer's saved context        */
    RavenCoroFn    fn;
    void          *arg;
    void          *transfer;  /* value in flight between resume/yield */
    RavenCoroState state;
};

/* Declare a 16-byte-aligned stack buffer for a coroutine. The RISC-V ABI
 * requires sp to be 16-byte aligned at function entry.
 *
 *   RAVEN_CORO_STACK(stack, 4096);
 */
#define RAVEN_CORO_STACK(name, size) \
    static char name[(size)] __attribute__((aligned(16)))

/* Prepare `co` to run `fn(co, arg)` on the stack [stack_base, stack_base+size).
 * Does not start the coroutine — the first raven_coro_resume does. The stack
 * buffer must outlive the coroutine and is owned by the caller. */
void raven_coro_init(RavenCoro *co, void *stack_base, raven_size_t stack_size,
                     RavenCoroFn fn, void *arg);

/* Resume `co`, passing `send` into the coroutine (it becomes the return value
 * of the raven_coro_yield that suspended it). Returns the value the coroutine
 * yields back, or NULL once the coroutine has finished. Resuming a finished or
 * already-running coroutine is a no-op that returns NULL. */
void *raven_coro_resume(RavenCoro *co, void *send);

/* Suspend the running coroutine `self`, handing `value` back to the resumer
 * (it becomes the return value of raven_coro_resume). Returns the value passed
 * to the next raven_coro_resume. Must be called from inside the coroutine body. */
void *raven_coro_yield(RavenCoro *self, void *value);

/* 1 if the coroutine's body has returned, 0 otherwise. */
int raven_coro_done(const RavenCoro *co);

/* Current lifecycle state. */
RavenCoroState raven_coro_status(const RavenCoro *co);

#endif

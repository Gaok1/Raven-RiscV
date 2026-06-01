#include <raven/coro.h>
#include <raven/debug.h>

/* Defined in lib/coro_switch.S. Saves callee-saved state into *from, restores
 * it from *to, then returns into the restored ra/sp. */
extern void _raven_coro_switch(_RavenCoroCtx *from, _RavenCoroCtx *to);
/* First-entry bootstrap in lib/coro_switch.S. Passes the primed s0 register
 * to _raven_coro_trampoline as the RavenCoro*. */
extern void _raven_coro_enter(void);

static void _raven_coro_zero_ctx(_RavenCoroCtx *c) {
    raven_u32   *w = (raven_u32 *)c;
    raven_size_t n = (raven_size_t)(sizeof(*c) / sizeof(raven_u32));
    for (raven_size_t i = 0; i < n; i++) w[i] = 0u;
}

/* Entered via `ret` from the first switch into a fresh coroutine: sp is the
 * coroutine's stack top and the body has never run yet. */
void _raven_coro_trampoline(RavenCoro *co) {
    co->fn(co, co->arg);
    co->state    = RAVEN_CORO_DONE;
    co->transfer = NULL;
    /* Hand control back to the resumer. The coroutine is DONE, so resume will
     * never switch back here. */
    _raven_coro_switch(&co->ctx, &co->caller);
    for (;;) { }   /* unreachable */
}

void raven_coro_init(RavenCoro *co, void *stack_base, raven_size_t stack_size,
                     RavenCoroFn fn, void *arg) {
    _raven_coro_zero_ctx(&co->ctx);
    _raven_coro_zero_ctx(&co->caller);
    co->fn       = fn;
    co->arg      = arg;
    co->transfer = NULL;
    co->state    = RAVEN_CORO_READY;

    /* sp = top of the buffer, 16-byte aligned (RISC-V ABI). ra = first-entry
     * bootstrap, which passes the primed s0 register as RavenCoro* to the
     * trampoline before tail-calling into the body. */
    raven_u32 top = (raven_u32)((char *)stack_base + stack_size);
    co->ctx.sp = top & ~15u;
    co->ctx.ra = (raven_u32)(raven_uintptr_t)&_raven_coro_enter;
    co->ctx.s[0] = (raven_u32)(raven_uintptr_t)co;
}

void *raven_coro_resume(RavenCoro *co, void *send) {
    if (co->state == RAVEN_CORO_DONE || co->state == RAVEN_CORO_RUNNING)
        return NULL;

    co->transfer = send;
    co->state    = RAVEN_CORO_RUNNING;

    /* Save the resumer's context, enter the coroutine. Control returns here
     * when the coroutine yields or its body returns. */
    _raven_coro_switch(&co->caller, &co->ctx);

    return co->transfer;
}

void *raven_coro_yield(RavenCoro *self, void *value) {
    if (!self) raven_panic("raven_coro_yield: not inside a coroutine");

    self->transfer = value;
    self->state    = RAVEN_CORO_SUSPENDED;

    /* Save the coroutine's context, return to the resumer. Control returns here
     * on the next resume. */
    _raven_coro_switch(&self->ctx, &self->caller);

    return self->transfer;
}

int raven_coro_done(const RavenCoro *co) {
    return co->state == RAVEN_CORO_DONE;
}

RavenCoroState raven_coro_status(const RavenCoro *co) {
    return co->state;
}

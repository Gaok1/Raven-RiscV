#include <raven/hart.h>
#include <raven/mem.h>
#include <raven/debug.h>
#include <raven/_ecall.h>
#include "internal/hart_payload.h"

static unsigned int _raven_stack_top(void *base, raven_size_t size) {
    unsigned int top = (unsigned int)((char *)base + size);
    return top & ~15u;     /* 16-byte ABI alignment */
}

static int _raven_sys_hart_start(unsigned int entry_pc,
                                 unsigned int sp,
                                 unsigned int arg) {
    register int          _a7 __asm__("a7") = RAVEN_ECALL_HART_START;
    register unsigned int _a0 __asm__("a0") = entry_pc;
    register unsigned int _a1 __asm__("a1") = sp;
    register unsigned int _a2 __asm__("a2") = arg;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2));
    return (int)_a0;
}

__attribute__((noreturn))
static void _raven_sys_hart_exit(void) {
    register int _a7 __asm__("a7") = RAVEN_ECALL_HART_EXIT;
    __asm__ volatile("ecall" :: "r"(_a7));
    __builtin_unreachable();
}

__attribute__((noreturn))
void _raven_hart_trampoline(unsigned int payload_ptr) {
    _RavenHartPayload *p = (_RavenHartPayload *)(raven_uintptr_t)payload_ptr;
    p->entry(p->arg);
    p->done = 1;
    if (p->self_free) raven_free(p);
    _raven_sys_hart_exit();
}

RavenHart raven_hart_spawn(RavenHartEntry entry,
                           void          *stack_base,
                           raven_size_t   stack_size,
                           unsigned int   arg) {
    _RavenHartPayload *p = (_RavenHartPayload *)raven_malloc(sizeof(_RavenHartPayload));
    if (!p) raven_panic("raven_hart_spawn: out of memory");
    p->entry     = entry;
    p->arg       = arg;
    p->done      = 0;
    p->self_free = 0;

    _raven_sys_hart_start(
        (unsigned int)(raven_uintptr_t)_raven_hart_trampoline,
        _raven_stack_top(stack_base, stack_size),
        (unsigned int)(raven_uintptr_t)p);

    RavenHart h = { p };
    return h;
}

RavenHart raven_hart_start(const RavenHartTask *task) {
    return raven_hart_spawn(task->entry, task->stack_base, task->stack_size, task->arg);
}

int raven_hart_is_done(RavenHart h) {
    return ((_RavenHartPayload *)h._payload)->done;
}

void raven_hart_join(RavenHart *h) {
    _RavenHartPayload *p = (_RavenHartPayload *)h->_payload;
    while (!p->done) { /* spin */ }
    raven_free(p);
    h->_payload = (void *)0;
}

void raven_hart_detach(RavenHart *h) {
    ((_RavenHartPayload *)h->_payload)->self_free = 1;
    h->_payload = (void *)0;
}

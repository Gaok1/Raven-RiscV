#include <raven/raven.h>

static void counter(RavenCoro *self, void *arg) {
    int base = (int)(raven_uintptr_t)arg;
    for (int i = 0; i < 3; i++) {
        raven_coro_yield(self, (void *)(raven_uintptr_t)(base + i));
    }
}

RAVEN_HART_STACK(worker0_stack, 4096);
RAVEN_HART_STACK(worker1_stack, 4096);
RAVEN_CORO_STACK(coro0_stack, 4096);
RAVEN_CORO_STACK(coro1_stack, 4096);

static void worker(unsigned int arg) {
    void *stack = arg == 0 ? (void *)coro0_stack : (void *)coro1_stack;
    void *base  = (void *)(raven_uintptr_t)(arg * 10);
    RavenCoro co;

    raven_coro_init(&co, stack, 4096, counter, base);
    while (!raven_coro_done(&co)) {
        void *v = raven_coro_resume(&co, NULL);
        if (!raven_coro_done(&co)) {
            raven_print_str("worker ");
            raven_print_uint(arg);
            raven_print_str(" yielded ");
            raven_print_int((int)(raven_uintptr_t)v);
            raven_println();
        }
    }
}

int main(void) {
    RavenHart h0 = RAVEN_HART_SPAWN(worker, worker0_stack, 0);
    RavenHart h1 = RAVEN_HART_SPAWN(worker, worker1_stack, 1);

    raven_hart_join(&h0);
    raven_hart_join(&h1);
    raven_println_str("done");
    return 0;
}

#include <raven/raven.h>

/* A generator: yields 1, 2, ..., n one value per resume, then returns. */
static void counter(RavenCoro *self, void *arg) {
    int n = (int) arg;
    for (int i = 1; i <= n; i++) {
        raven_coro_yield(self, (void *)(raven_uintptr_t)i);
    }
}

int main(void) {
    RAVEN_CORO_STACK(stack, 4096);

    RavenCoro co;
    raven_coro_init(&co, stack, sizeof(stack), counter, (void*)5);

    while (!raven_coro_done(&co)) {
        
        void *v = raven_coro_resume(&co, NULL);

        if (!raven_coro_done(&co)) {
            raven_printf("yielded %d\n", (int)(raven_uintptr_t)v);
        }
    }

    raven_printf("done\n");
    return 0;
}

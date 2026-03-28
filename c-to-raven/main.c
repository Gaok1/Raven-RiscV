#include "raven.h"

// ── Workers ───────────────────────────────────────────────────────────────────

RAVEN_HART_STACK(sum_stack,   4096);
RAVEN_HART_STACK(count_stack, 4096);

static void sum_worker(unsigned int n) {
    unsigned int sum = 0;
    for (unsigned int i = 1; i <= n; i++) sum += i;
    raven_print_str("sum 1..");
    raven_print_uint(n);
    raven_print_str(" = ");
    raven_print_uint(sum);
    raven_print_newline();
}

static void count_worker(unsigned int start) {
    raven_print_str("counting: ");
    for (unsigned int i = start; i < start + 5; i++) {
        raven_print_uint(i);
        if (i < start + 4) raven_print_str(", ");
    }
    raven_print_newline();
}

// ── Entry point ───────────────────────────────────────────────────────────────

int main(void) {
    print_str("=== c-to-raven hart demo ===\n\n");

    // ── Pattern 1: task descriptor + join ─────────────────────────────────────
    RavenHartTask task = raven_hart_task_array(sum_worker, sum_stack, 100);
    RavenHartHandle handle = raven_hart_task_start(&task);

    print_str("main hart: waiting for sum_worker...\n");
    handle.join(&handle);
    print_str("main hart: sum_worker done.\n\n");

    // ── Pattern 2: quick spawn + poll ─────────────────────────────────────────
    RavenHartHandle h2 = raven_spawn_hart_array(count_worker, count_stack, 10);

    print_str("main hart: count_worker running...\n");

    while (!h2.is_finished(&h2)) { print_str("Spinning...");}

    print_str("main hart: count_worker finished.\n");

    __sys_exit(0);
    return 0;
}

// main.c — example: malloc, string ops, sorting, and raven_pause()
//
// Demonstrates:
//   - malloc / calloc / free from raven.h
//   - memset, memcpy, strlen, strcmp
//   - print_hex, print_ptr
//   - raven_assert
//
// Single-step with Raven's [Dyn] view (press v until "DYN" appears in the
// status bar) to watch each sw/lw flip the sidebar between memory and registers.

#include "raven.h"

#define N 16

// ── helpers ──────────────────────────────────────────────────────────────────

static void fill_random(int *a, int n, int limit) {
    unsigned int seed;
    sys_getrandom(&seed, 4, 0);
    for (int i = 0; i < n; i++) {
        seed = seed * 1664525u + 1013904223u; // LCG
        a[i] = (int)(seed % (unsigned int)limit);
    }
}

static void bubble_sort(int *a, int n) {
    for (int i = 0; i < n - 1; i++)
        for (int j = 0; j < n - i - 1; j++)
            if (a[j] > a[j + 1]) {
                int tmp = a[j]; a[j] = a[j + 1]; a[j + 1] = tmp;
            }
}

static void print_array(const char *label, const int *a, int n) {
    print_str(label);
    print_str(" [");
    for (int i = 0; i < n; i++) {
        if (i > 0) print_str(", ");
        print_int(a[i]);
    }
    print_str("]\n");
}

// ── string demo ──────────────────────────────────────────────────────────────

static void demo_strings(void) {
    print_str("\n--- String utilities ---\n");

    char buf[32];
    strcpy(buf, "Hello");
    strcat(buf, ", Raven!");
    print_str("strcpy+strcat: "); print_str(buf); print_ln();
    print_str("strlen:        "); print_uint((unsigned int)strlen(buf)); print_ln();
    print_str("strcmp equal:  "); print_bool(strcmp(buf, "Hello, Raven!") == 0); print_ln();
    print_str("strchr ',' :   ");
    char *p = strchr(buf, ',');
    print_str(p ? p : "(null)"); print_ln();
}

// ── malloc demo ──────────────────────────────────────────────────────────────

static void demo_malloc(void) {
    print_str("\n--- Heap allocator ---\n");
    print_str("heap free before: "); print_uint((unsigned int)raven_heap_free()); print_str(" B\n");

    // Allocate array on the heap
    int *arr = (int *)malloc(N * sizeof(int));
    raven_assert(arr != NULL);

    print_str("arr @ "); print_ptr(arr); print_ln();

    fill_random(arr, N, 100);
    print_array("unsorted:", arr, N);

    bubble_sort(arr, N);
    print_array("sorted:  ", arr, N);

    // calloc — zero-initialised
    int *zeros = (int *)calloc(8, sizeof(int));
    raven_assert(zeros != NULL);
    int all_zero = 1;
    for (int i = 0; i < 8; i++) if (zeros[i] != 0) { all_zero = 0; break; }
    print_str("calloc zeroed:    "); print_bool(all_zero); print_ln();

    // realloc — grow the zero buffer
    zeros = (int *)realloc(zeros, 16 * sizeof(int));
    raven_assert(zeros != NULL);

    free(zeros);
    free(arr);

    print_str("heap free after:  "); print_uint((unsigned int)raven_heap_free()); print_str(" B\n");
}

// ── main ─────────────────────────────────────────────────────────────────────

int main(void) {
    print_str("=== c-to-raven example ===\n");

    demo_strings();
    demo_malloc();

    print_str("\nDone. Pausing for inspection...\n");
    raven_pause(); // freeze here — inspect heap/regs in Raven before exit
    return 0;
}

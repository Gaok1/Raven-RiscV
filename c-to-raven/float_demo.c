// float_demo.c — hardware float via RV32F (fadd.s, fmul.s, fcvt.w.s, ...)
// Compiled with -march=rv32imf -mabi=ilp32f  →  make float-demo
//
// Open Raven's Run tab, press Tab on the register sidebar to switch to
// float registers, and watch f0–f31 update in real time as you single-step.
// In Dyn mode (v → v → v) the sidebar automatically flips to show the
// memory written by every fsw and the registers changed by every arithmetic op.

#include "raven.h"

static void fill_floats(float *a, int n) {
    float v = 1.0f;
    for (int i = 0; i < n; i++) {
        a[i] = v;
        v += 1.5f;
    }
}

static float sum_floats(const float *a, int n) {
    float s = 0.0f;
    for (int i = 0; i < n; i++) s += a[i];
    return s;
}

static float dot_product(const float *a, const float *b, int n) {
    float s = 0.0f;
    for (int i = 0; i < n; i++) s += a[i] * b[i];
    return s;
}

#define N 8

static float a[N];
static float b[N];

int main(void) {
    print_str("Float demo — RV32F hardware instructions\n\n");

    fill_floats(a, N);
    fill_floats(b, N);

    // Reverse b so the dot product is more interesting
    for (int i = 0; i < N / 2; i++) {
        float tmp = b[i]; b[i] = b[N - 1 - i]; b[N - 1 - i] = tmp;
    }

    print_str("a = [");
    for (int i = 0; i < N; i++) {
        if (i > 0) print_str(", ");
        print_float(a[i], 3);
    }
    print_str("]\n");

    print_str("b = [");
    for (int i = 0; i < N; i++) {
        if (i > 0) print_str(", ");
        print_float(b[i], 3);
    }
    print_str("]\n\n");

    float s = sum_floats(a, N);
    print_str("sum(a)    = "); print_float(s, 3); print_ln();

    float d = dot_product(a, b, N);
    print_str("dot(a,b)  = "); print_float(d, 3); print_ln();

    float pi  = 3.14159f;
    float tau = pi * 2.0f;
    print_str("pi * 2    = "); print_float(tau, 5); print_ln();

    print_str("\nDone.\n");
    raven_pause();
    return 0;
}

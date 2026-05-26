#include <raven/raven.h>

static void fill_floats(float *a, int n) {
    for (int i = 0; i < n; i++)
        a[i] = (float)i + 0.25f * (float)i;
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
    raven_print_str("Float demo - RV32F hardware instructions\n\n");

    fill_floats(a, N);
    fill_floats(b, N);
    for (int i = 0; i < N / 2; i++) {
        float t = b[i]; b[i] = b[N - 1 - i]; b[N - 1 - i] = t;
    }

    float s = sum_floats(a, N);
    raven_print_str("sum(a)    = "); raven_print_float_n(s, 3); raven_println();

    float d = dot_product(a, b, N);
    raven_print_str("dot(a,b)  = "); raven_print_float_n(d, 3); raven_println();

    float pi = 3.14159f;
    float tau = pi * 2.0f;
    raven_print_str("pi * 2    = "); raven_print_float_n(tau, 5); raven_println();

    raven_print_str("\nDone.\n");
    return 0;
}

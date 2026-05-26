#ifndef RAVEN_MATH_H
#define RAVEN_MATH_H

#include <raven/types.h>

static inline int          raven_abs  (int n)                          { return n < 0 ? -n : n; }
static inline int          raven_min  (int a, int b)                   { return a < b ? a : b; }
static inline int          raven_max  (int a, int b)                   { return a > b ? a : b; }
static inline unsigned int raven_umin (unsigned int a, unsigned int b) { return a < b ? a : b; }
static inline unsigned int raven_umax (unsigned int a, unsigned int b) { return a > b ? a : b; }
static inline int          raven_clamp(int v, int lo, int hi)          { return v < lo ? lo : (v > hi ? hi : v); }

static inline int raven_ipow(int base, unsigned int exp) {
    int r = 1;
    while (exp--) r *= base;
    return r;
}

#endif

#include <raven/rand.h>
#include <raven/_ecall.h>

static int _raven_getrandom(void *buf, int len) {
    register int   _a7 __asm__("a7") = RAVEN_ECALL_GETRANDOM;
    register void *_a0 __asm__("a0") = buf;
    register int   _a1 __asm__("a1") = len;
    register int   _a2 __asm__("a2") = 0;     /* no flags */
    int ret;
    __asm__ volatile("ecall" : "=r"(ret)
                              : "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2)
                              : "memory");
    return ret;
}

raven_u32 raven_rand_u32(void) {
    raven_u32 v;
    _raven_getrandom(&v, (int)sizeof(v));
    return v;
}

raven_u8 raven_rand_u8(void) {
    raven_u8 v;
    _raven_getrandom(&v, 1);
    return v;
}

int raven_rand_i32(void)  { return (int)raven_rand_u32(); }
int raven_rand_bool(void) { return (int)(raven_rand_u8() & 1u); }

unsigned int raven_rand_range(unsigned int lo, unsigned int hi) {
    if (hi <= lo) return lo;
    return lo + raven_rand_u32() % (hi - lo);
}

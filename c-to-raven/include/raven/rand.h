#ifndef RAVEN_RAND_H
#define RAVEN_RAND_H

#include <raven/types.h>

/* Cryptographic random bytes (backed by the getrandom ecall).
 * Defined in libraven.a. */

raven_u32    raven_rand_u32  (void);
raven_u8     raven_rand_u8   (void);
int          raven_rand_i32  (void);
int          raven_rand_bool (void);
unsigned int raven_rand_range(unsigned int lo, unsigned int hi);  /* [lo, hi) */

#endif

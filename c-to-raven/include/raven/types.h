#ifndef RAVEN_TYPES_H
#define RAVEN_TYPES_H

/* Raven canonical fixed-width integer types. */
typedef unsigned char       raven_u8;
typedef unsigned short      raven_u16;
typedef unsigned int        raven_u32;
typedef unsigned long long  raven_u64;
typedef signed char         raven_i8;
typedef signed short        raven_i16;
typedef signed int          raven_i32;
typedef signed long long    raven_i64;
typedef raven_u32           raven_size_t;
typedef raven_i32           raven_ssize_t;
typedef raven_u32           raven_uintptr_t;
typedef raven_i32           raven_ptrdiff_t;

/* Libc-standard aliases. Provided for ergonomics so generic C idioms compile
 * cleanly under -nostdlib. The raven_-prefixed names are canonical; these
 * are convenience aliases only. */
#ifndef NULL
#define NULL ((void *)0)
#endif

typedef raven_size_t    size_t;
typedef raven_ptrdiff_t ptrdiff_t;

#endif

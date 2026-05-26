#ifndef RAVEN_DEBUG_H
#define RAVEN_DEBUG_H

#include <raven/types.h>

/* Write a fatal message to stderr and exit with code 1. Does NOT pause for
 * inspection; if you want that, call raven_unsafe_breakpoint() from
 * <raven/advanced.h> first. */
__attribute__((noreturn)) void raven_panic(const char *msg);

/* Terminate the program with the given exit code. */
__attribute__((noreturn)) void raven_exit(int code);

/* Internal: backs raven_assert. */
__attribute__((noreturn)) void raven_assert_fail(const char *expr,
                                                 const char *file, int line);

/* raven_assert — if expr is false, prints
 *     ASSERT failed: <expr> at <file>:<line>
 * to stderr and exits with code 1. */
#define raven_assert(expr) \
    do { if (!(expr)) raven_assert_fail(#expr, __FILE__, __LINE__); } while (0)

#endif

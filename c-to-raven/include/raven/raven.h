#ifndef RAVEN_H
#define RAVEN_H

/* ─────────────────────────────────────────────────────────────────────────
 *  raven.h — Raven C SDK (RISC-V 32-bit, bare metal, -nostdlib)
 *
 *  Include this single header and you get the full ergonomic API.
 *  For low-level escape hatches (raw ecalls, ebreak, JIT exec-mapping)
 *  additionally:   #include <raven/advanced.h>
 *
 *  Namespace policy. Raven reserves these prefixes — do not define your
 *  own identifiers in them:
 *      raven_*, Raven*, RAVEN_*, _raven_*
 *
 *  Modules included by this umbrella:
 *      <raven/types.h>   integer types, size_t, NULL
 *      <raven/math.h>    abs / min / max / clamp / ipow
 *      <raven/rand.h>    rand_u32 / rand_range / rand_bool / ...
 *      <raven/str.h>     strlen / strcmp / strcpy / ...
 *      <raven/mem.h>     malloc / free / memset / memcpy / ...
 *      <raven/io.h>      print / read / println / eprint
 *      <raven/fmt.h>     printf / scanf / snprintf / sscanf
 *      <raven/hart.h>    multi-hart spawn / join / detach
 *      <raven/coro.h>    cooperative stackful coroutines (resume / yield)
 *      <raven/perf.h>    instr_count / cycle_count / RAVEN_MEASURE
 *      <raven/debug.h>   assert / panic / exit
 *      <raven/version.h> RAVEN_API_VERSION
 * ───────────────────────────────────────────────────────────────────────── */

#include <raven/version.h>
#include <raven/types.h>
#include <raven/math.h>
#include <raven/rand.h>
#include <raven/str.h>
#include <raven/mem.h>
#include <raven/io.h>
#include <raven/fmt.h>
#include <raven/hart.h>
#include <raven/coro.h>
#include <raven/perf.h>
#include <raven/debug.h>

#endif

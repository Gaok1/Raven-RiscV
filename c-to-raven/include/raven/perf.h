#ifndef RAVEN_PERF_H
#define RAVEN_PERF_H

#include <raven/types.h>
#include <raven/_ecall.h>
#include <raven/io.h>   /* for RAVEN_MEASURE expansion */

/* 64-bit retired-instruction count (a0 = low, a1 = high). */
static inline raven_u64 raven_instr_count(void) {
    register int       _a7 __asm__("a7") = RAVEN_ECALL_INSTR_COUNT;
    register raven_u32 _a0 __asm__("a0");
    register raven_u32 _a1 __asm__("a1");
    __asm__ volatile("ecall" : "=r"(_a0), "=r"(_a1) : "r"(_a7));
    return ((raven_u64)_a1 << 32) | (raven_u64)_a0;
}

/* 64-bit cycle count (includes cache penalties when caches are enabled). */
static inline raven_u64 raven_cycle_count(void) {
    register int       _a7 __asm__("a7") = RAVEN_ECALL_CYCLE_COUNT;
    register raven_u32 _a0 __asm__("a0");
    register raven_u32 _a1 __asm__("a1");
    __asm__ volatile("ecall" : "=r"(_a0), "=r"(_a1) : "r"(_a7));
    return ((raven_u64)_a1 << 32) | (raven_u64)_a0;
}

static inline raven_u32 raven_instr_count32(void) { return (raven_u32)raven_instr_count(); }
static inline raven_u32 raven_cycle_count32(void) { return (raven_u32)raven_cycle_count(); }

/* RAVEN_MEASURE(label, { ...block... })
 *
 * Runs the block, then prints:
 *     <label>: <N> instr, <M> cycles
 *
 * Example:
 *     RAVEN_MEASURE("bubble sort", {
 *         bubble_sort(arr, N);
 *     });
 */
#define RAVEN_MEASURE(label, block) do {                                      \
    raven_u64 _raven_m_i0 = raven_instr_count();                              \
    raven_u64 _raven_m_c0 = raven_cycle_count();                              \
    block                                                                     \
    raven_u64 _raven_m_i1 = raven_instr_count();                              \
    raven_u64 _raven_m_c1 = raven_cycle_count();                              \
    raven_print_str(label);                                                   \
    raven_print_str(": ");                                                    \
    raven_print_uint((raven_u32)(_raven_m_i1 - _raven_m_i0));                 \
    raven_print_str(" instr, ");                                              \
    raven_print_uint((raven_u32)(_raven_m_c1 - _raven_m_c0));                 \
    raven_println_str(" cycles");                                             \
} while (0)

#endif

#ifndef RAVEN_FMT_H
#define RAVEN_FMT_H

#include <raven/types.h>

/* ─────────────────────────────────────────────────────────────────────────
 *  Formatted I/O — printf / scanf family.
 *
 *  These functions perform formatting in C (not in the simulator), so the
 *  work counts toward raven_instr_count. If you need a single-ecall print
 *  that stays invisible to benchmarks, use the typed helpers in
 *  <raven/io.h> (raven_print_int / raven_print_str / ...).
 *
 *  raven_printf   writes to stdout via one buffered write ecall per
 *                 128-byte flush (formatting cost is C-side).
 *  raven_snprintf formats into a caller-supplied buffer (printf-into-a-
 *                 string). Bounded: at most `size - 1` bytes plus NUL.
 *  raven_scanf    reads from stdin one byte at a time (one ecall per byte).
 *  raven_sscanf   parses from a NUL-terminated string — no ecalls at all.
 *
 *  Supported conversions:
 *      %d %i  signed decimal (int)
 *      %u     unsigned decimal (raven_u32)
 *      %x %X  unsigned hex
 *      %o     unsigned octal
 *      %b     unsigned binary  (Raven extension)
 *      %c     char
 *      %s     NUL-terminated string
 *      %p     pointer, printed as "0xHHHHHHHH"
 *      %%     literal '%'
 *
 *  Floats are intentionally not supported. Variadic float promotion to
 *  double would pull in compiler-rt soft-float helpers that -nostdlib
 *  strips; use raven_print_float / raven_print_float_n from <raven/io.h>
 *  for the float parts of your output.
 *
 *  Supported modifiers:
 *      flags     '-'  '+'  ' '  '0'  '#'
 *      width     decimal digit sequence (no '*')
 *      precision '.' followed by decimal digits
 *      length    h / hh / l / ll / z / j / t  — accepted and ignored
 *                (int and long are both 32-bit on rv32; pass a raven_u64
 *                as two raven_u32 args if you need 64-bit values).
 *
 *  scanf-only modifiers:
 *      '*' to suppress assignment (parse and discard).
 * ───────────────────────────────────────────────────────────────────────── */

/* va_list type — bound to the compiler builtin so user code doesn't have
 * to include <stdarg.h>. raven_va_list is identical to the standard
 * va_list under any conforming toolchain, so they can be used
 * interchangeably. */
typedef __builtin_va_list raven_va_list;

/* ── Output ─────────────────────────────────────────────────────────────── */

int raven_printf  (const char *fmt, ...)
    __attribute__((format(printf, 1, 2)));
int raven_vprintf (const char *fmt, raven_va_list ap);

/* Formats into `buf`. Always NUL-terminates if `size > 0`. Returns the
 * number of bytes that would have been written if `size` had been
 * sufficient (excluding the NUL) — matches C99 snprintf semantics, so
 * truncation is detectable via `(return_value >= size)`. */
int raven_snprintf (char *buf, raven_size_t size, const char *fmt, ...)
    __attribute__((format(printf, 3, 4)));
int raven_vsnprintf(char *buf, raven_size_t size,
                    const char *fmt, raven_va_list ap);

/* ── Input ──────────────────────────────────────────────────────────────── */

int raven_scanf  (const char *fmt, ...)
    __attribute__((format(scanf, 1, 2)));
int raven_vscanf (const char *fmt, raven_va_list ap);

int raven_sscanf (const char *str, const char *fmt, ...)
    __attribute__((format(scanf, 2, 3)));
int raven_vsscanf(const char *str, const char *fmt, raven_va_list ap);

#endif

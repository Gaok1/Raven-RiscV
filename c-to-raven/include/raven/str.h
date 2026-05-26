#ifndef RAVEN_STR_H
#define RAVEN_STR_H

#include <raven/types.h>
#include <raven/_ecall.h>

/* ── Simulator-accelerated (single ecall — work invisible to instr_count) ─ */

static inline raven_size_t raven_strlen(const char *s) {
    register int          _a7 __asm__("a7") = RAVEN_ECALL_STRLEN;
    register const char  *_a0 __asm__("a0") = s;
    raven_u32 ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0));
    return (raven_size_t)ret;
}

static inline int raven_strcmp(const char *a, const char *b) {
    register int          _a7 __asm__("a7") = RAVEN_ECALL_STRCMP;
    register const char  *_a0 __asm__("a0") = a;
    register const char  *_a1 __asm__("a1") = b;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0), "r"(_a1));
    return ret;
}

/* ── Pure C-loop variants ────────────────────────────────────────────────────
 * Use these when you want the work to count toward instr_count (benchmarking
 * cache behavior, branch prediction, etc.). Defined in libraven.a. */

raven_size_t raven_strlen_c(const char *s);
int          raven_strcmp_c(const char *a, const char *b);

/* ── Rest of the string family (C only — no ecall variant exists) ───────── */

int   raven_strncmp(const char *a, const char *b, raven_size_t n);
char *raven_strcpy (char *dst, const char *src);
char *raven_strncpy(char *dst, const char *src, raven_size_t n);
char *raven_strcat (char *dst, const char *src);
char *raven_strchr (const char *s, int c);
char *raven_strrchr(const char *s, int c);

#endif

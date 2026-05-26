#ifndef RAVEN_ADVANCED_H
#define RAVEN_ADVANCED_H

/* ╔════════════════════════════════════════════════════════════════════════╗
 * ║  <raven/advanced.h> — LOW-LEVEL ESCAPE HATCHES                         ║
 * ║                                                                        ║
 * ║  Everything in this header bypasses Raven's safety and ergonomics.     ║
 * ║  Use only when you have a specific reason. Misuse silently corrupts    ║
 * ║  memory or hangs the hart.                                             ║
 * ║                                                                        ║
 * ║  Names beginning with `raven_sys_` map directly to a single ecall and  ║
 * ║  follow the Linux RISC-V ABI. Names beginning with `raven_unsafe_`     ║
 * ║  perform operations whose effects depend on the simulator's internal   ║
 * ║  state (memory map, JIT, hart scheduler).                              ║
 * ╚════════════════════════════════════════════════════════════════════════╝ */

#include <raven/types.h>
#include <raven/_ecall.h>

/* ── File descriptors ────────────────────────────────────────────────────── */
#define RAVEN_FD_STDIN  0
#define RAVEN_FD_STDOUT 1
#define RAVEN_FD_STDERR 2

/* ── mmap protection / mapping flags ─────────────────────────────────────── */
#define RAVEN_PROT_NONE     0x00
#define RAVEN_PROT_READ     0x01
#define RAVEN_PROT_WRITE    0x02
#define RAVEN_PROT_EXEC     0x04
#define RAVEN_MAP_SHARED    0x01
#define RAVEN_MAP_PRIVATE   0x02
#define RAVEN_MAP_ANONYMOUS 0x20

typedef struct RavenIovec {
    void     *iov_base;
    raven_u32 iov_len;
} RavenIovec;

typedef struct RavenTimespec {
    raven_u32 tv_sec;
    raven_u32 tv_nsec;
} RavenTimespec;

/* ── Direct Linux syscall wrappers ───────────────────────────────────────── */

static inline int raven_sys_write(int fd, const void *buf, int len) {
    register int         _a7 __asm__("a7") = RAVEN_ECALL_WRITE;
    register int         _a0 __asm__("a0") = fd;
    register const void *_a1 __asm__("a1") = buf;
    register int         _a2 __asm__("a2") = len;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret)
                              : "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2)
                              : "memory");
    return ret;
}

static inline int raven_sys_read(int fd, void *buf, int len) {
    register int   _a7 __asm__("a7") = RAVEN_ECALL_READ;
    register int   _a0 __asm__("a0") = fd;
    register void *_a1 __asm__("a1") = buf;
    register int   _a2 __asm__("a2") = len;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret)
                              : "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2)
                              : "memory");
    return ret;
}

static inline int raven_sys_writev(int fd, const RavenIovec *iov, int iovcnt) {
    register int               _a7 __asm__("a7") = RAVEN_ECALL_WRITEV;
    register int               _a0 __asm__("a0") = fd;
    register const RavenIovec *_a1 __asm__("a1") = iov;
    register int               _a2 __asm__("a2") = iovcnt;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret)
                              : "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2)
                              : "memory");
    return ret;
}

__attribute__((noreturn))
static inline void raven_sys_exit(int code) {
    register int _a7 __asm__("a7") = RAVEN_ECALL_EXIT;
    register int _a0 __asm__("a0") = code;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
    __builtin_unreachable();
}

static inline int raven_sys_getpid(void) {
    register int _a7 __asm__("a7") = RAVEN_ECALL_GETPID;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7));
    return ret;
}

static inline int raven_sys_getuid(void) {
    register int _a7 __asm__("a7") = RAVEN_ECALL_GETUID;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7));
    return ret;
}

static inline int raven_sys_getgid(void) {
    register int _a7 __asm__("a7") = RAVEN_ECALL_GETGID;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7));
    return ret;
}

static inline void *raven_sys_brk(void *addr) {
    register int   _a7 __asm__("a7") = RAVEN_ECALL_BRK;
    register void *_a0 __asm__("a0") = addr;
    void *ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0));
    return ret;
}

static inline void *raven_sys_mmap(void *addr, raven_size_t len, int prot,
                                   int flags, int fd, int offset) {
    register int          _a7 __asm__("a7") = RAVEN_ECALL_MMAP;
    register void        *_a0 __asm__("a0") = addr;
    register raven_size_t _a1 __asm__("a1") = len;
    register int          _a2 __asm__("a2") = prot;
    register int          _a3 __asm__("a3") = flags;
    register int          _a4 __asm__("a4") = fd;
    register int          _a5 __asm__("a5") = offset;
    void *ret;
    __asm__ volatile("ecall" : "=r"(ret)
                              : "r"(_a7), "r"(_a0), "r"(_a1),
                                "r"(_a2), "r"(_a3), "r"(_a4), "r"(_a5)
                              : "memory");
    return ret;
}

static inline int raven_sys_munmap(void *addr, raven_size_t len) {
    register int          _a7 __asm__("a7") = RAVEN_ECALL_MUNMAP;
    register void        *_a0 __asm__("a0") = addr;
    register raven_size_t _a1 __asm__("a1") = len;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0), "r"(_a1));
    return ret;
}

static inline int raven_sys_getrandom(void *buf, int len, unsigned int flags) {
    register int          _a7 __asm__("a7") = RAVEN_ECALL_GETRANDOM;
    register void        *_a0 __asm__("a0") = buf;
    register int          _a1 __asm__("a1") = len;
    register unsigned int _a2 __asm__("a2") = flags;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret)
                              : "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2)
                              : "memory");
    return ret;
}

static inline int raven_sys_clock_gettime(int clockid, RavenTimespec *tp) {
    register int            _a7 __asm__("a7") = RAVEN_ECALL_CLOCK_GETTIME;
    register int            _a0 __asm__("a0") = clockid;
    register RavenTimespec *_a1 __asm__("a1") = tp;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0), "r"(_a1) : "memory");
    return ret;
}

/* ── Unsafe primitives ───────────────────────────────────────────────────── */

/* Trigger an EBREAK exception. The simulator pauses and exposes registers /
 * memory in the UI; the user can step or resume. This is a *resumable*
 * breakpoint, not an exit. */
static inline void raven_unsafe_breakpoint(void) {
    __asm__ volatile("ebreak");
}

/* Mark [addr, addr+len) as executable memory. Required after writing
 * instruction bytes if you intend to jump into them while running with
 * --jit=hot or --jit=full; otherwise the JIT will not pick them up. Both
 * addr and len must be 4-byte aligned. Returns 0 on success, -EINVAL on bad
 * arguments. */
static inline int raven_unsafe_map_exec(void *addr, raven_size_t len) {
    register int          _a7 __asm__("a7") = RAVEN_ECALL_MAP_EXEC;
    register void        *_a0 __asm__("a0") = addr;
    register raven_size_t _a1 __asm__("a1") = len;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0), "r"(_a1));
    return ret;
}

/* Exit only the current hart (program continues on other harts). Normally
 * the trampoline in <raven/hart.h> calls this for you when a hart's entry
 * function returns. Calling it directly skips payload cleanup. */
__attribute__((noreturn))
static inline void raven_unsafe_hart_exit(void) {
    register int _a7 __asm__("a7") = RAVEN_ECALL_HART_EXIT;
    __asm__ volatile("ecall" :: "r"(_a7));
    __builtin_unreachable();
}

/* Low-level hart start: raw entry PC, raw stack pointer, raw arg. The
 * trampoline-and-payload machinery built on top of this lives in
 * <raven/hart.h> — use that unless you really need raw control.
 * Returns hart_id (>= 1) on success, -1 if no free core,
 * -2 if entry_pc is outside an executable region. */
static inline int raven_unsafe_hart_start(unsigned int entry_pc,
                                          unsigned int stack_ptr,
                                          unsigned int arg) {
    register int          _a7 __asm__("a7") = RAVEN_ECALL_HART_START;
    register unsigned int _a0 __asm__("a0") = entry_pc;
    register unsigned int _a1 __asm__("a1") = stack_ptr;
    register unsigned int _a2 __asm__("a2") = arg;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2));
    return (int)_a0;
}

#endif

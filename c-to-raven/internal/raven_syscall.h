#pragma once

// Internal syscall and simulator-control layer for raven.h.

static inline int __sys_write(int fd, const void *buf, int len) {
    register int         _a7 __asm__("a7") = SYS_WRITE;
    register int         _a0 __asm__("a0") = fd;
    register const void *_a1 __asm__("a1") = buf;
    register int         _a2 __asm__("a2") = len;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2));
    return _a0;
}

static inline int __sys_read(int fd, void *buf, int len) {
    register int   _a7 __asm__("a7") = SYS_READ;
    register int   _a0 __asm__("a0") = fd;
    register void *_a1 __asm__("a1") = buf;
    register int   _a2 __asm__("a2") = len;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2));
    return _a0;
}

__attribute__((noreturn))
static inline void __sys_exit(int code) {
    register int _a7 __asm__("a7") = SYS_EXIT;
    register int _a0 __asm__("a0") = code;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
    __builtin_unreachable();
}

__attribute__((noreturn))
static inline void __sys_exit_group(int code) {
    register int _a7 __asm__("a7") = 94;
    register int _a0 __asm__("a0") = code;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
    __builtin_unreachable();
}

static inline int __sys_getrandom(void *buf, int len, unsigned int flags) {
    register int          _a7 __asm__("a7") = SYS_GETRANDOM;
    register void        *_a0 __asm__("a0") = buf;
    register int          _a1 __asm__("a1") = len;
    register unsigned int _a2 __asm__("a2") = flags;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2));
    return ret;
}

static inline void *__sys_brk(void *addr) {
    register int   _a7 __asm__("a7") = SYS_BRK;
    register void *_a0 __asm__("a0") = addr;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7));
    return _a0;
}

static inline int __sys_getpid(void) {
    register int _a7 __asm__("a7") = SYS_GETPID;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7));
    return ret;
}

static inline int __sys_getuid(void) {
    register int _a7 __asm__("a7") = SYS_GETUID;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7));
    return ret;
}

static inline int __sys_getgid(void) {
    register int _a7 __asm__("a7") = SYS_GETGID;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7));
    return ret;
}

typedef struct {
    void        *iov_base;
    unsigned int iov_len;
} raven_iovec;

static inline int __sys_writev(int fd, const raven_iovec *iov, int iovcnt) {
    register int                _a7 __asm__("a7") = SYS_WRITEV;
    register int                _a0 __asm__("a0") = fd;
    register const raven_iovec *_a1 __asm__("a1") = iov;
    register int                _a2 __asm__("a2") = iovcnt;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2));
    return _a0;
}

static inline void *__sys_mmap(void *addr, size_t len, int prot, int flags, int fd, int offset) {
    register int   _a7 __asm__("a7") = SYS_MMAP;
    register void *_a0 __asm__("a0") = addr;
    register int   _a1 __asm__("a1") = (int)len;
    register int   _a2 __asm__("a2") = prot;
    register int   _a3 __asm__("a3") = flags;
    register int   _a4 __asm__("a4") = fd;
    register int   _a5 __asm__("a5") = offset;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2), "r"(_a3), "r"(_a4), "r"(_a5));
    return _a0;
}

static inline int __sys_munmap(void *addr, size_t len) {
    register int   _a7 __asm__("a7") = SYS_MUNMAP;
    register void *_a0 __asm__("a0") = addr;
    register int   _a1 __asm__("a1") = (int)len;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1));
    return (int)(size_t)_a0;
}

typedef struct {
    unsigned int tv_sec;
    unsigned int tv_nsec;
} raven_timespec;

static inline int __sys_clock_gettime(int clockid, raven_timespec *tp) {
    register int             _a7 __asm__("a7") = SYS_CLOCK_GETTIME;
    register int             _a0 __asm__("a0") = clockid;
    register raven_timespec *_a1 __asm__("a1") = tp;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1));
    return _a0;
}

static inline void raven_pause(void) {
    __asm__ volatile("ebreak");
}

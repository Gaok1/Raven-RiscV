#include <raven/debug.h>
#include <raven/_ecall.h>

static int _raven_write(int fd, const void *buf, int len) {
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

__attribute__((noreturn))
static void _raven_sys_exit(int code) {
    register int _a7 __asm__("a7") = RAVEN_ECALL_EXIT;
    register int _a0 __asm__("a0") = code;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
    __builtin_unreachable();
}

static raven_size_t _strlen_local(const char *s) {
    raven_size_t n = 0; while (*s++) n++; return n;
}

__attribute__((noreturn))
void raven_panic(const char *msg) {
    static const char head[] = "PANIC: ";
    _raven_write(2, head, (int)(sizeof(head) - 1));
    _raven_write(2, msg, (int)_strlen_local(msg));
    _raven_write(2, "\n", 1);
    _raven_sys_exit(1);
}

__attribute__((noreturn))
void raven_exit(int code) {
    _raven_sys_exit(code);
}

__attribute__((noreturn))
void raven_assert_fail(const char *expr, const char *file, int line) {
    static const char head[] = "ASSERT failed: ";
    _raven_write(2, head, (int)(sizeof(head) - 1));
    _raven_write(2, expr, (int)_strlen_local(expr));
    _raven_write(2, " at ", 4);
    _raven_write(2, file, (int)_strlen_local(file));
    _raven_write(2, ":", 1);

    char buf[12]; int i = 11; buf[i] = '\0';
    raven_u32 n = (raven_u32)line;
    if (n == 0) {
        _raven_write(2, "0", 1);
    } else {
        while (n > 0) { buf[--i] = (char)('0' + (char)(n % 10)); n /= 10; }
        _raven_write(2, buf + i, (int)(11 - i));
    }
    _raven_write(2, "\n", 1);
    _raven_sys_exit(1);
}

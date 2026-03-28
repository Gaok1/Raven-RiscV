#pragma once

// Internal Raven teaching-extension layer for raven.h.

static inline raven_u64 __raven_u64_from_a0_a1(unsigned int lo, unsigned int hi) {
    return ((raven_u64)hi << 32) | (raven_u64)lo;
}

static inline void raven_print_int(int n) {
    register int _a7 __asm__("a7") = SYS_RAVEN_PRINT_INT;
    register int _a0 __asm__("a0") = n;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_print_str(const char *s) {
    register int         _a7 __asm__("a7") = SYS_RAVEN_PRINT_STR;
    register const char *_a0 __asm__("a0") = s;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_println_str(const char *s) {
    register int         _a7 __asm__("a7") = SYS_RAVEN_PRINTLN_STR;
    register const char *_a0 __asm__("a0") = s;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_read_line(char *buf) {
    register int   _a7 __asm__("a7") = SYS_RAVEN_READ_LINE;
    register char *_a0 __asm__("a0") = buf;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void __raven_read_u8_ptr(unsigned char *dst) {
    register int            _a7 __asm__("a7") = SYS_RAVEN_READ_U8;
    register unsigned char *_a0 __asm__("a0") = dst;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void __raven_read_u16_ptr(unsigned short *dst) {
    register int             _a7 __asm__("a7") = SYS_RAVEN_READ_U16;
    register unsigned short *_a0 __asm__("a0") = dst;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void __raven_read_u32_ptr(unsigned int *dst) {
    register int           _a7 __asm__("a7") = SYS_RAVEN_READ_U32;
    register unsigned int *_a0 __asm__("a0") = dst;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_print_uint(unsigned int n) {
    register int          _a7 __asm__("a7") = SYS_RAVEN_PRINT_UINT;
    register unsigned int _a0 __asm__("a0") = n;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_print_hex(unsigned int n) {
    register int          _a7 __asm__("a7") = SYS_RAVEN_PRINT_HEX;
    register unsigned int _a0 __asm__("a0") = n;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_print_char(char c) {
    register int _a7 __asm__("a7") = SYS_RAVEN_PRINT_CHAR;
    register int _a0 __asm__("a0") = (unsigned char)c;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_print_newline(void) {
    register int _a7 __asm__("a7") = SYS_RAVEN_PRINT_NEWLINE;
    __asm__ volatile("ecall" :: "r"(_a7));
}

static inline void __raven_read_int_ptr(int *dst) {
    register int  _a7 __asm__("a7") = SYS_RAVEN_READ_INT;
    register int *_a0 __asm__("a0") = dst;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void __raven_read_float_ptr(float *dst) {
    register int    _a7 __asm__("a7") = SYS_RAVEN_READ_FLOAT;
    register float *_a0 __asm__("a0") = dst;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

// ── Public read API (value-returning) ────────────────────────────────────────

static inline int raven_read_int(void) {
    int v; __raven_read_int_ptr(&v); return v;
}

static inline unsigned int raven_read_uint(void) {
    unsigned int v; __raven_read_u32_ptr(&v); return v;
}

static inline float raven_read_float(void) {
    float v; __raven_read_float_ptr(&v); return v;
}

static inline unsigned char raven_read_u8(void) {
    unsigned char v; __raven_read_u8_ptr(&v); return v;
}

static inline unsigned short raven_read_u16(void) {
    unsigned short v; __raven_read_u16_ptr(&v); return v;
}

static inline unsigned int raven_read_u32(void) {
    unsigned int v; __raven_read_u32_ptr(&v); return v;
}

static inline void raven_print_float(float v) {
    union {
        float v;
        unsigned int bits;
    } bits = { v };
    register int          _a7 __asm__("a7") = SYS_RAVEN_PRINT_FLOAT;
    register unsigned int _a0 __asm__("a0") = bits.bits;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0));
}

static inline void raven_print_bool(int v) {
    raven_print_str(v ? "true" : "false");
}

static inline void raven_print_ptr(const void *p) {
    raven_print_hex((unsigned int)(size_t)p);
}

static inline void raven_print_bin(unsigned int n) {
    for (int i = 31; i >= 0; i--) {
        raven_print_char('0' + (char)((n >> i) & 1));
        if (i > 0 && i % 8 == 0) raven_print_char(' ');
    }
}

static inline raven_u64 raven_get_instr_count(void) {
    register int          _a7 __asm__("a7") = SYS_RAVEN_GET_INSTR_COUNT;
    register unsigned int _a0 __asm__("a0");
    register unsigned int _a1 __asm__("a1");
    __asm__ volatile("ecall" : "=r"(_a0), "=r"(_a1) : "r"(_a7));
    return __raven_u64_from_a0_a1(_a0, _a1);
}

static inline raven_u64 raven_get_cycle_count(void) {
    register int          _a7 __asm__("a7") = SYS_RAVEN_GET_CYCLE_COUNT;
    register unsigned int _a0 __asm__("a0");
    register unsigned int _a1 __asm__("a1");
    __asm__ volatile("ecall" : "=r"(_a0), "=r"(_a1) : "r"(_a7));
    return __raven_u64_from_a0_a1(_a0, _a1);
}

static inline unsigned int raven_get_instr_count32(void) {
    return (unsigned int)raven_get_instr_count();
}

static inline unsigned int raven_get_cycle_count32(void) {
    return (unsigned int)raven_get_cycle_count();
}

static inline void raven_memset(void *dst, unsigned char byte, size_t len) {
    register int          _a7 __asm__("a7") = SYS_RAVEN_MEMSET;
    register void        *_a0 __asm__("a0") = dst;
    register unsigned int _a1 __asm__("a1") = (unsigned int)byte;
    register int          _a2 __asm__("a2") = (int)len;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2));
}

static inline void raven_memcpy(void *dst, const void *src, size_t len) {
    register int         _a7 __asm__("a7") = SYS_RAVEN_MEMCPY;
    register void       *_a0 __asm__("a0") = dst;
    register const void *_a1 __asm__("a1") = src;
    register int         _a2 __asm__("a2") = (int)len;
    __asm__ volatile("ecall" :: "r"(_a7), "r"(_a0), "r"(_a1), "r"(_a2));
}

static inline size_t raven_strlen(const char *s) {
    register int         _a7 __asm__("a7") = SYS_RAVEN_STRLEN;
    register const char *_a0 __asm__("a0") = s;
    unsigned int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0));
    return (size_t)ret;
}

static inline int raven_strcmp(const char *s1, const char *s2) {
    register int         _a7 __asm__("a7") = SYS_RAVEN_STRCMP;
    register const char *_a0 __asm__("a0") = s1;
    register const char *_a1 __asm__("a1") = s2;
    int ret;
    __asm__ volatile("ecall" : "=r"(ret) : "r"(_a7), "r"(_a0), "r"(_a1));
    return ret;
}

typedef void (*raven_hart_fn)(unsigned int arg);

// ── Internal payload ──────────────────────────────────────────────────────────
// Heap-allocated by the start functions.
// Freed by join(), or by the trampoline itself when self_free is set (detach).

typedef struct {
    raven_hart_fn entry;
    unsigned int  arg;
    volatile int  done;
    int           self_free;
} __RavenHartPayload;

// ── RavenHartHandle ───────────────────────────────────────────────────────────
// Returned by raven_hart_task_start() and raven_spawn_hart().
//
// Methods embedded in the struct:
//   h.is_finished(&h)  — returns 1 when the hart has exited, 0 while running
//   h.join(&h)         — spin-wait until done, then free internal resources
//   h.detach(&h)       — abandon: hart frees itself on exit, handle becomes invalid

typedef struct RavenHartHandle RavenHartHandle;

static inline int  __raven_hart_handle_is_finished(const RavenHartHandle *h);
static inline void __raven_hart_handle_join(RavenHartHandle *h);
static inline void __raven_hart_handle_detach(RavenHartHandle *h);

struct RavenHartHandle {
    __RavenHartPayload *__p;
    int  (*is_finished)(const RavenHartHandle *h);
    void (*join)(RavenHartHandle *h);
    void (*detach)(RavenHartHandle *h);
};

static inline int __raven_hart_handle_is_finished(const RavenHartHandle *h) {
    return h->__p->done;
}

static inline void __raven_hart_handle_join(RavenHartHandle *h) {
    while (!h->__p->done) { /* spin */ }
    free(h->__p);
    h->__p = NULL;
}

static inline void __raven_hart_handle_detach(RavenHartHandle *h) {
    h->__p->self_free = 1;
    h->__p = NULL;
}

// Free-function aliases — same behaviour as the method calls above.
static inline int  raven_hart_handle_is_finished(const RavenHartHandle *h) { return __raven_hart_handle_is_finished(h); }
static inline void raven_hart_handle_join(RavenHartHandle *h)               { __raven_hart_handle_join(h); }
static inline void raven_hart_handle_detach(RavenHartHandle *h)             { __raven_hart_handle_detach(h); }

// Declare a 16-byte-aligned stack buffer for use with hart spawn functions.
// The RISC-V ABI requires the initial stack pointer to be 16-byte aligned.
//
// Usage:
//   RAVEN_HART_STACK(my_stack, 4096);
//   RavenHartHandle h = raven_spawn_hart_array(fn, my_stack, arg);
#define RAVEN_HART_STACK(name, size) \
    static char name[(size)] __attribute__((aligned(16)))

// ── RavenHartTask ─────────────────────────────────────────────────────────────
// Describes a hart before launching it.
// Create with raven_hart_task() or raven_hart_task_array(), then call
// raven_hart_task_start() to launch and get a handle.

typedef struct {
    raven_hart_fn entry;
    void         *stack_base;
    size_t        stack_size;
    unsigned int  arg;
} RavenHartTask;

static inline RavenHartTask raven_hart_task(raven_hart_fn entry,
                                            void         *stack_base,
                                            size_t        stack_size,
                                            unsigned int  arg) {
    RavenHartTask t;
    t.entry      = entry;
    t.stack_base = stack_base;
    t.stack_size = stack_size;
    t.arg        = arg;
    return t;
}

// Create a RavenHartTask from a stack array — size computed automatically.
#define raven_hart_task_array(fn_ptr, stack_arr, arg_value) \
    raven_hart_task((fn_ptr), (stack_arr), sizeof(stack_arr), (unsigned int)(arg_value))

// ── Internal helpers ──────────────────────────────────────────────────────────

static inline unsigned int __raven_stack_top(void *base, size_t size) {
    unsigned int top = (unsigned int)((char *)base + size);
    return top & ~15u;  // RISC-V ABI requires 16-byte aligned SP
}

static inline int __sys_hart_start(unsigned int entry_pc,
                                   unsigned int stack_ptr,
                                   unsigned int arg) {
    register int          _a7 __asm__("a7") = SYS_RAVEN_HART_START;
    register unsigned int _a0 __asm__("a0") = entry_pc;
    register unsigned int _a1 __asm__("a1") = stack_ptr;
    register unsigned int _a2 __asm__("a2") = arg;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2));
    return (int)_a0;
}

__attribute__((noreturn))
static inline void __sys_hart_exit(void) {
    register int _a7 __asm__("a7") = SYS_RAVEN_HART_EXIT;
    __asm__ volatile("ecall" :: "r"(_a7));
    __builtin_unreachable();
}

__attribute__((noreturn))
static void __raven_hart_trampoline(unsigned int payload_ptr) {
    __RavenHartPayload *p = (__RavenHartPayload *)(size_t)payload_ptr;
    p->entry(p->arg);
    p->done = 1;
    if (p->self_free) free(p);
    __sys_hart_exit();
}

static inline RavenHartHandle __raven_start_hart(raven_hart_fn entry,
                                                 unsigned int  arg,
                                                 void         *stack_base,
                                                 size_t        stack_size) {
    __RavenHartPayload *p = (__RavenHartPayload *)malloc(sizeof(__RavenHartPayload));
    if (!p) raven_panic("raven_spawn_hart: out of memory");
    p->entry     = entry;
    p->arg       = arg;
    p->done      = 0;
    p->self_free = 0;
    __sys_hart_start(
        (unsigned int)(size_t)__raven_hart_trampoline,
        __raven_stack_top(stack_base, stack_size),
        (unsigned int)(size_t)p
    );
    RavenHartHandle h;
    h.__p         = p;
    h.is_finished = __raven_hart_handle_is_finished;
    h.join        = __raven_hart_handle_join;
    h.detach      = __raven_hart_handle_detach;
    return h;
}

// ── Public hart API ───────────────────────────────────────────────────────────

// Launch a task descriptor.  Returns a handle.
static inline RavenHartHandle raven_hart_task_start(const RavenHartTask *task) {
    return __raven_start_hart(task->entry, task->arg, task->stack_base, task->stack_size);
}

// Spawn a hart from a function pointer.  Returns a handle.
static inline RavenHartHandle raven_spawn_hart(raven_hart_fn entry,
                                               void         *stack_base,
                                               size_t        stack_size,
                                               unsigned int  arg) {
    return __raven_start_hart(entry, arg, stack_base, stack_size);
}

// Spawn from a stack array — size computed automatically.
#define raven_spawn_hart_array(fn_ptr, stack_arr, arg) \
    raven_spawn_hart((fn_ptr), (stack_arr), sizeof(stack_arr), (unsigned int)(arg))

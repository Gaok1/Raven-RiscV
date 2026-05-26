# coro_switch.S — cooperative coroutine context switch for Raven (RV32)
#
#   void _raven_coro_switch(_RavenCoroCtx *from, _RavenCoroCtx *to);
#       a0 = from : save the current callee-saved state here
#       a1 = to   : restore callee-saved state from here, then `ret`
#
# `ret` jumps to the restored `ra` with the restored `sp`, which is how a
# coroutine resumes exactly where it last suspended. Only callee-saved
# registers are saved — the compiler already spills caller-saved registers
# (a0-a7, t*, ft*) around the call to this function.
#
# _RavenCoroCtx layout — these offsets MUST match include/raven/coro.h:
#     0: ra        4: sp
#     8: s0  (x8) 12: s1  (x9) 16: s2 ... 52: s11 (x27)   # 12 words
#    56: fs0 (f8) ...        100: fs11 (f27)              # 12 words, F only
#
# The fs0-fs11 block is assembled only when the F extension is present, so the
# same source is correct for both the rv32im (soft-float) and rv32imf
# (hard-float) archives. __riscv_flen is defined (== 32) only for rv32imf.

    .section .text._raven_coro_switch
    .globl   _raven_coro_switch
    .type    _raven_coro_switch, @function
_raven_coro_switch:
    # ── save current context into *a0 ───────────────────────────────────────
    sw   ra,    0(a0)
    sw   sp,    4(a0)
    sw   s0,    8(a0)
    sw   s1,   12(a0)
    sw   s2,   16(a0)
    sw   s3,   20(a0)
    sw   s4,   24(a0)
    sw   s5,   28(a0)
    sw   s6,   32(a0)
    sw   s7,   36(a0)
    sw   s8,   40(a0)
    sw   s9,   44(a0)
    sw   s10,  48(a0)
    sw   s11,  52(a0)
#if defined(__riscv_flen)
    fsw  fs0,  56(a0)
    fsw  fs1,  60(a0)
    fsw  fs2,  64(a0)
    fsw  fs3,  68(a0)
    fsw  fs4,  72(a0)
    fsw  fs5,  76(a0)
    fsw  fs6,  80(a0)
    fsw  fs7,  84(a0)
    fsw  fs8,  88(a0)
    fsw  fs9,  92(a0)
    fsw  fs10, 96(a0)
    fsw  fs11,100(a0)
#endif

    # ── restore next context from *a1 ───────────────────────────────────────
    lw   ra,    0(a1)
    lw   sp,    4(a1)
    lw   s0,    8(a1)
    lw   s1,   12(a1)
    lw   s2,   16(a1)
    lw   s3,   20(a1)
    lw   s4,   24(a1)
    lw   s5,   28(a1)
    lw   s6,   32(a1)
    lw   s7,   36(a1)
    lw   s8,   40(a1)
    lw   s9,   44(a1)
    lw   s10,  48(a1)
    lw   s11,  52(a1)
#if defined(__riscv_flen)
    flw  fs0,  56(a1)
    flw  fs1,  60(a1)
    flw  fs2,  64(a1)
    flw  fs3,  68(a1)
    flw  fs4,  72(a1)
    flw  fs5,  76(a1)
    flw  fs6,  80(a1)
    flw  fs7,  84(a1)
    flw  fs8,  88(a1)
    flw  fs9,  92(a1)
    flw  fs10, 96(a1)
    flw  fs11,100(a1)
#endif
    ret
    .size _raven_coro_switch, .-_raven_coro_switch

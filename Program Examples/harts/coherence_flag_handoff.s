# Cache coherence — a value handed from one core to another through a flag.
#
# Core 0 writes 42 to `data`, then raises `flag`. Core 1 spins on `flag`,
# copies `data` to `result` and raises `done`. Core 0 waits for `done` and
# exits with `result`. Each word sits on its own 16-byte cache line.
#
# Run it with two cores and each coherence setting:
#
#   raven run coherence_flag_handoff.s --cores 2 --coherence shared   # exit 42
#   raven run coherence_flag_handoff.s --cores 2 --coherence none --max-cycles 200000
#   raven run coherence_flag_handoff.s --cores 2 --coherence msi  --format csv
#   raven run coherence_flag_handoff.s --cores 2 --coherence mesi --format csv
#   raven run coherence_flag_handoff.s --cores 2 --coherence moesi --format csv
#
# In the TUI: Settings → max cores 2, run scope ALL, coherence <protocol>;
# then watch Cache → coherence.
#
# With `none` the run never ends: core 1 read `flag` (0) into its private L1
# before core 0 raised it, and keeps reading that copy, while core 0's 1 sits
# dirty in core 0's own L1. A protocol makes core 0's write invalidate core
# 1's copy, so core 1's next read misses and gets the 1.

.data
.align 4
data:   .word 0
.align 4
flag:   .word 0
.align 4
result: .word 0
.align 4
done:   .word 0

.text
_start:
    # Start core 1 at `consumer`, with a stack 4 KiB below ours.
    li   t5, -4096
    add  a1, sp, t5
    la   a0, consumer
    li   a2, 0
    li   a7, 1100            # hart_start
    ecall

    la   t0, data
    li   t1, 42
    sw   t1, 0(t0)           # data = 42
    la   t0, flag
    li   t1, 1
    sw   t1, 0(t0)           # flag = 1
wait:
    la   t0, done
    lw   t1, 0(t0)
    beqz t1, wait            # until done != 0
    la   t0, result
    lw   a0, 0(t0)
    li   a7, 93              # exit(result)
    ecall

consumer:
spin:
    la   t0, flag
    lw   t1, 0(t0)
    beqz t1, spin            # until flag != 0
    la   t0, data
    lw   t1, 0(t0)
    la   t0, result
    sw   t1, 0(t0)           # result = data
    la   t0, done
    li   t1, 1
    sw   t1, 0(t0)           # done = 1
    li   a7, 1101            # hart_exit
    ecall

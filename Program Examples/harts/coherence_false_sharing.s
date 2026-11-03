# Cache coherence — false sharing.
#
# Two cores each add 1 to a counter of their own, 200 times. Neither core
# ever reads the other's counter, yet when the two counters share a cache
# line every write by one core invalidates the line in the other core's L1:
# the line ping-pongs between the caches. That is false sharing.
#
#   raven run coherence_false_sharing.s --cores 2 --coherence mesi --format csv
#
# Look at the COHERENCE table: BusUpgr/BusRdX, invalidations and coherence
# misses on both cores. Then change STRIDE below from 4 to 16, so the
# counters land on separate 16-byte lines, and run it again: the traffic
# all but disappears and the cycle count drops. Exit code = 0 when both
# counters reached 200.
#
# In the TUI: Settings → max cores 2, run scope ALL, coherence MESI; then
# Cache → coherence shows the same counters live.

.data
.align 6
counters: .space 64

.text
_start:
    li   t5, -4096
    add  a1, sp, t5
    la   a0, worker
    li   a2, 4               # STRIDE: byte offset of core 1's counter (4 = same line, 16 = next line)
    li   a7, 1100            # hart_start, a0 of the new hart = STRIDE
    ecall

    la   t0, counters        # core 0 counts at offset 0
    li   t2, 200
loop0:
    lw   t1, 0(t0)
    addi t1, t1, 1
    sw   t1, 0(t0)
    addi t2, t2, -1
    bnez t2, loop0

    # Wait until core 1's counter reaches 200 too (it sits at offset 4 or 16).
    li   t3, 200
    la   t4, counters
    addi t5, t4, 4
    addi t6, t4, 16
wait:
    lw   t1, 0(t5)
    beq  t1, t3, done
    lw   t1, 0(t6)
    beq  t1, t3, done
    j    wait
done:
    lw   t1, 0(t0)
    sub  a0, t1, t3          # 0 when core 0's counter is 200
    li   a7, 93
    ecall

worker:
    la   t0, counters
    add  t0, t0, a0          # core 1 counts at offset STRIDE
    li   t2, 200
loop1:
    lw   t1, 0(t0)
    addi t1, t1, 1
    sw   t1, 0(t0)
    addi t2, t2, -1
    bnez t2, loop1
    li   a7, 1101            # hart_exit
    ecall

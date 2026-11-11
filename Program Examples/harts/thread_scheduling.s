# Thread scheduling — three workers and main share the cores.
#
# Main starts three workers (A, B, C) and waits for each with thread_join.
# Every worker does four rounds of busy work and prints its letter after
# each round. Which letters come out in which order is the scheduler's doing:
#
#   raven run thread_scheduling.s --cores 1 --sched fifo           # AAAABBBBCCCC
#   raven run thread_scheduling.s --cores 1 --sched rr --quantum 20 # ABCABCABCABC
#   raven run thread_scheduling.s --cores 1 --sched rr --quantum 200
#   raven run thread_scheduling.s --cores 2 --sched rr --quantum 20
#   raven run thread_scheduling.s --cores 1 --sched rr --quantum 20 --switch-cost 10 --format csv
#
# Without a scheduler (--sched off, the default) a worker needs a core of its
# own: with --cores 4 all three run at once, with --cores 1 hart_start fails.
#
# In the TUI: Settings → scheduler rr, quantum 20, max cores 1; run, then
# watch the Threads tab.

.data
.align 4
stacks: .space 12288          # three stacks of 4 KiB
tids:   .word 0, 0, 0
done:   .asciz "done"

.text
_start:
    li   s0, 0                # worker index
    la   s1, tids
spawn:
    la   a0, worker           # entry PC
    la   a1, stacks
    addi t0, s0, 1
    slli t0, t0, 12
    add  a1, a1, t0           # stack top: stacks + (index + 1) * 4096
    mv   a2, s0               # argument: the worker's index
    li   a7, 1100             # hart_start
    ecall                     # a0 = thread id
    slli t0, s0, 2
    add  t0, s1, t0
    sw   a0, 0(t0)            # tids[index] = id
    addi s0, s0, 1
    li   t0, 3
    blt  s0, t0, spawn

    li   s0, 0
join:
    slli t0, s0, 2
    add  t0, s1, t0
    lw   a0, 0(t0)
    li   a7, 1104             # thread_join(tids[index])
    ecall
    addi s0, s0, 1
    li   t0, 3
    blt  s0, t0, join

    li   a7, 1008             # print_newline
    ecall
    la   a0, done
    li   a7, 1001             # print_zstr
    ecall
    li   a7, 1008             # print_newline
    ecall
    li   a0, 0
    li   a7, 93               # exit(0)
    ecall

worker:
    addi s1, a0, 65           # 'A' + index
    li   s2, 4                # rounds
round:
    li   t0, 40               # busy work: 40 iterations
busy:
    addi t0, t0, -1
    bnez t0, busy
    mv   a0, s1
    li   a7, 1006             # print_char
    ecall
    addi s2, s2, -1
    bnez s2, round
    li   a7, 1101             # hart_exit
    ecall

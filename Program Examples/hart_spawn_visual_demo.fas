.data
msg_boot:      .asciz "multi-hart visual demo boot\n"
msg_h1_done:   .asciz "hart 1 done\n"
msg_h2_done:   .asciz "hart 2 done\n"
msg_h3_done:   .asciz "hart 3 done\n"
msg_main_done: .asciz "main hart done\n"

.align 4
counter_main: .word 0
counter_h1:   .word 0
counter_h2:   .word 0
counter_h3:   .word 0

.text
_start:
    la   a0, msg_boot
    li   a7, 1002
    ecall

    # Requires Max Cores >= 4.
    # This demo assumes RAM >= 320KB so each hart stack slot clamps to 64KB.
    # Child stacks are derived from the current SP so the same file works in
    # the CLI default RAM size and in a larger TUI RAM configuration.
    li   t5, -4096
    add  t5, sp, t5

    la   a0, hart1
    li   t6, 0x00010000
    sub  a1, t5, t6
    li   a2, 1
    li   a7, 1100
    ecall

    la   a0, hart2
    li   t6, 0x00020000
    sub  a1, t5, t6
    li   a2, 2
    li   a7, 1100
    ecall

    la   a0, hart3
    li   t6, 0x00030000
    sub  a1, t5, t6
    li   a2, 3
    li   a7, 1100
    ecall

    li   t0, 120
main_loop:
    la   t1, counter_main
    lw   t2, 0(t1)
    addi t2, t2, 1
    sw   t2, 0(t1)
    addi t0, t0, -1
    bnez t0, main_loop

    la   a0, msg_main_done
    li   a7, 1002
    ecall
    halt

hart1:
    li   t0, 80
h1_loop:
    la   t1, counter_h1
    lw   t2, 0(t1)
    addi t2, t2, 1
    sw   t2, 0(t1)
    addi t0, t0, -1
    bnez t0, h1_loop
    la   a0, msg_h1_done
    li   a7, 1002
    ecall
    halt

hart2:
    li   t0, 64
    li   t3, 3
    li   t4, 11
h2_loop:
    remu t5, t4, t3
    addi t5, t5, 1
    la   t1, counter_h2
    lw   t2, 0(t1)
    add  t2, t2, t5
    sw   t2, 0(t1)
    addi t4, t4, 7
    addi t0, t0, -1
    bnez t0, h2_loop
    la   a0, msg_h2_done
    li   a7, 1002
    ecall
    halt

hart3:
    li   t0, 96
    li   t3, 0
h3_loop:
    addi t3, t3, 1
    andi t4, t3, 1
    beqz t4, h3_even
    la   t1, counter_h3
    lw   t2, 0(t1)
    addi t2, t2, 3
    sw   t2, 0(t1)
    j    h3_next
h3_even:
    la   t1, counter_h3
    lw   t2, 0(t1)
    addi t2, t2, 1
    sw   t2, 0(t1)
h3_next:
    addi t0, t0, -1
    bnez t0, h3_loop
    la   a0, msg_h3_done
    li   a7, 1002
    ecall
    halt

##! D501 — Dois cores com estados independentes
##! Configure pelo menos 2 cores e compare PC, x1 e x5.
##! Os cores compartilham memoria, mas nao o banco de registradores.

.data
main_msg:  .asciz "Core 0 terminou\n"
child_msg: .asciz "Core 1 terminou\n"
shared:    .word 0

.text
_start:
##! Core 0 cria o Core 1 e depois segue com registradores proprios
    li   t5, -4096
    add  t5, sp, t5

    la   a0, core1_entry
    li   t6, 0x00010000
    sub  a1, t5, t6
    li   a2, 1
    li   a7, 1100
    ecall

    li   x1, 111
    li   x5, 555
    li   s0, 80

core0_loop:
    la   t0, shared
    lw   t1, 0(t0)
    addi t1, t1, 1
    sw   t1, 0(t0)
    addi s0, s0, -1
    bnez s0, core0_loop

    la   a0, main_msg
    li   a7, 1002
    ecall
    halt

core1_entry:
##! Core 1 comeca aqui, com valores diferentes em x1 e x5
    li   x1, 222
    li   x5, 999
    li   s1, 50

core1_loop:
    la   t0, shared
    lw   t1, 0(t0)
    addi t1, t1, 2
    sw   t1, 0(t0)
    addi s1, s1, -1
    bnez s1, core1_loop

    la   a0, child_msg
    li   a7, 1002
    ecall
    halt

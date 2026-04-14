##! D101 — Instruções independentes
##! Todas as instruções do loop usam registradores fixos e independentes.
##! Compare o CPI deste arquivo com o D102.

.data
msg: .asciz "D101 — CPI esperado: ~1,0 (sem dependencias RAW)"

.text
    printStrLn msg

    li   t0, 5
    li   t1, 12
    li   t2, 7
    li   t3, 19
    li   t4, 3
    li   t5, 8
    li   s0, 60        # 60 iterações

loop:
    add  a0, t0, t1    #! sem RAW com a instrucao anterior
    xor  a1, t2, t3    #! sem RAW com a instrucao anterior
    add  a2, t4, t5    #! sem RAW com a instrucao anterior
    xor  a3, t0, t3    #! sem RAW com a instrucao anterior
    add  a4, t1, t2    #! sem RAW com a instrucao anterior
    xor  a5, t4, t0    #! sem RAW com a instrucao anterior
    addi s0, s0, -1
    bnez s0, loop

    li   a0, 0
    li   a7, 93
    ecall

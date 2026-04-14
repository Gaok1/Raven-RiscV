##! D301 — Streaming em array
##! Use este arquivo para comparar AMAT e tambem LRU vs FIFO.
##! O acesso e sequencial, com pouca localidade temporal de curto prazo.

.data
array: .space 512      # 128 palavras (512 bytes)

.text
##! Fase 1 — preenchimento do array
    la   t0, array
    li   t1, 128       # número de elementos
    li   t2, 0         # índice

fill:
    sw   t2, 0(t0)
    addi t0, t0, 4
    addi t2, t2, 1
    blt  t2, t1, fill

##! Fase 2 — leitura sequencial repetida
    li   s1, 8         # número de passagens
    li   s0, 0         # acumulador

pass:
    la   t0, array
    li   t2, 0

scan:
    lw   t3, 0(t0)
    add  s0, s0, t3
    addi t0, t0, 4
    addi t2, t2, 1
    blt  t2, t1, scan

    addi s1, s1, -1
    bnez s1, pass

    li   a0, 0
    li   a7, 93
    ecall

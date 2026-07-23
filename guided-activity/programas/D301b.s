##! D301b — AMAT: working set de 640 B
##! C311 (256 B) nao comporta o array inteiro → miss rate alto
##! C312 (1 KB) comporta o array inteiro → hit rate alto apos o warm-up
##! Compare os AMATs nas duas configuracoes.

.data
array: .space 640        # 640 B = 160 palavras

.text
##! Fase 1 — preenchimento sequencial do array
    la   t0, array
    li   t1, 160
    li   t2, 0

fill:
    sw   t2, 0(t0)
    addi t0, t0, 4
    addi t2, t2, 1
    blt  t2, t1, fill

##! Fase 2 — 2 passagens de leitura sequencial
    li   s1, 2
    li   s0, 0

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

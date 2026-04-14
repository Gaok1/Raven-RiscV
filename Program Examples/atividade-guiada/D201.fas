##! D201 — Load-use hazard
##! Primeiro rode como esta. Depois insira um nop entre o lw e o add.
##! Observe a diferenca entre bolha automatica e reordenacao manual.

.data
valores: .word 3, 7, 11, 15, 19

.text
    la   t0, valores   # ponteiro para o início do array
    li   t5, 5         # 5 elementos

loop:
    lw   t1, 0(t0)     #! valor so fica pronto ao fim de MEM
    add  t2, t1, t1    #! uso imediato de t1 -> stall inevitavel
    sw   t2, 0(t0)     #! escrita do resultado
    addi t0, t0, 4     # avança ponteiro (próxima palavra)
    addi t5, t5, -1
    bnez t5, loop

    li   a0, 0
    li   a7, 93
    ecall

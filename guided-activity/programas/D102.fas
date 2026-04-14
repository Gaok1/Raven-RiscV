##! D102 — Cadeia de dependências RAW
##! Cada instrucao le o valor produzido pela anterior.
##! Use este arquivo tanto no D1 quanto no D6.

.data
msg: .asciz "D102 — CPI esperado: >1,5 (cadeia de dependencias RAW)"

.text
    printStrLn msg

    li   s0, 40        # 40 iterações
    li   t0, 1         # valor inicial da cadeia

loop:
    add  t1, t0, t0    #! inicia a cadeia
    add  t2, t1, t1    #! RAW em t1
    add  t3, t2, t2    #! RAW em t2
    xor  t4, t3, t2    #! RAW em t3
    add  t5, t4, t3    #! RAW em t4
    add  t0, t5, t1    #! RAW em t5
    addi s0, s0, -1
    bnez s0, loop

    li   a0, 0
    li   a7, 93
    ecall

##! D401 — I, R, M e S
##! Na aba Run, mova o cursor sobre as instrucoes para ver a decodificacao no painel direito.
##! Os tipos aparecem em blocos contiguos para facilitar a comparacao.

.text

    addi x2, x0, 6     #! I-type
    addi x3, x0, 7     #! I-type

    add  x4, x2, x3    #! R-type: [11:7] = rd
    sub  x5, x4, x2    #! R-type: [11:7] = rd

    mul  x6, x4, x5    #! M-type
    rem  x7, x6, x3    #! M-type

    sw   x6, 0(x1)     #! S-type: [11:7] = imediato[4:0]
    sw   x7, 4(x1)     #! S-type: [11:7] = imediato[4:0]

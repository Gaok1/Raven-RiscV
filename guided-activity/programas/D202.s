##! D202 — Flush por redirecionamento de controle
##! O primeiro beq e tomado, mas com predict=NotTaken o caminho sequencial
##! abaixo e buscado antes da resolucao e deve ser descartado.
##! Compare este comportamento com o stall do D201.

.data
msg_ok: .asciz "Flush correto: s1 = 0"

.text
    li   s0, 1         # valor para forcar o branch tomado
    li   s1, 0         # contador do caminho errado (deve ficar 0)

    beq  s0, s0, taken     #! tomado -> predict=NotTaken causa flush
    addi s1, s1, 99        #! caminho errado: deve ser descartado
    addi s1, s1, 1         #! segunda instrucao errada para deixar o flush visivel

taken:
    printStrLn msg_ok
    li   a0, 0
    li   a7, 93
    ecall

# Benchmark: soma de array de 1 000 000 elementos
# x5 = contador (N=1000000)
# x6 = acumulador
# x7 = valor a somar (constante 1)
.text
    li   x5, 1000000      # N
    li   x6, 0            # sum = 0
    li   x7, 1            # val = 1
loop:
    add  x6, x6, x7       # sum += val
    addi x5, x5, -1       # n--
    bne  x5, x0, loop     # if n != 0 goto loop
    halt

# Benchmark: fibonacci iterativo até N=500000 vezes
# Calcula fib(30) em loop para forçar execução repetida
# x5 = loop counter
# x6, x7 = fib(a), fib(b)
# x8 = temp
# x9 = n para fib interno
.text
    li   x5, 500000       # repeat 500000 vezes

repeat:
    li   x6, 0            # a = 0
    li   x7, 1            # b = 1
    li   x9, 30           # n = 30

fib_loop:
    add  x8, x6, x7       # temp = a + b
    mv   x6, x7           # a = b
    mv   x7, x8           # b = temp
    addi x9, x9, -1       # n--
    bne  x9, x0, fib_loop # if n != 0 goto fib_loop

    addi x5, x5, -1       # repeat--
    bne  x5, x0, repeat   # if repeat != 0 goto repeat

    halt

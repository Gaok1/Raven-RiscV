# Benchmark: bubble sort de 200 elementos (0..199 em ordem reversa)
# Layout de memória: array em 0x1000, 200 words de 4 bytes = 800 bytes
# Após o sort o array deve estar em ordem crescente (0..199)
#
# Registradores:
#   x10 = base do array
#   x11 = n (tamanho)
#   x12 = i (loop externo)
#   x13 = j (loop interno)
#   x14 = arr[j]
#   x15 = arr[j+1]
.data
array: .word 199, 198, 197, 196, 195, 194, 193, 192, 191, 190
       .word 189, 188, 187, 186, 185, 184, 183, 182, 181, 180
       .word 179, 178, 177, 176, 175, 174, 173, 172, 171, 170
       .word 169, 168, 167, 166, 165, 164, 163, 162, 161, 160
       .word 159, 158, 157, 156, 155, 154, 153, 152, 151, 150
       .word 149, 148, 147, 146, 145, 144, 143, 142, 141, 140
       .word 139, 138, 137, 136, 135, 134, 133, 132, 131, 130
       .word 129, 128, 127, 126, 125, 124, 123, 122, 121, 120
       .word 119, 118, 117, 116, 115, 114, 113, 112, 111, 110
       .word 109, 108, 107, 106, 105, 104, 103, 102, 101, 100
       .word  99,  98,  97,  96,  95,  94,  93,  92,  91,  90
       .word  89,  88,  87,  86,  85,  84,  83,  82,  81,  80
       .word  79,  78,  77,  76,  75,  74,  73,  72,  71,  70
       .word  69,  68,  67,  66,  65,  64,  63,  62,  61,  60
       .word  59,  58,  57,  56,  55,  54,  53,  52,  51,  50
       .word  49,  48,  47,  46,  45,  44,  43,  42,  41,  40
       .word  39,  38,  37,  36,  35,  34,  33,  32,  31,  30
       .word  29,  28,  27,  26,  25,  24,  23,  22,  21,  20
       .word  19,  18,  17,  16,  15,  14,  13,  12,  11,  10
       .word   9,   8,   7,   6,   5,   4,   3,   2,   1,   0

.text
    la   x10, array       # base
    li   x11, 200         # n = 200

outer:
    li   x12, 0           # i = 0
    addi x11, x11, -1     # n--
    beq  x11, x0, done    # if n == 0 -> done

inner:
    bge  x12, x11, outer  # if i >= n -> next outer pass
    slli x13, x12, 2      # offset = i * 4
    add  x13, x10, x13    # ptr = base + offset
    lw   x14, 0(x13)      # arr[i]
    lw   x15, 4(x13)      # arr[i+1]
    ble  x14, x15, no_swap
    sw   x15, 0(x13)      # swap
    sw   x14, 4(x13)
no_swap:
    addi x12, x12, 1      # i++
    j    inner

done:
    halt

<img src="https://github.com/user-attachments/assets/b0a9c716-3750-4aba-85f0-6957d2b510fc" height="500"/>

# Falcon ASM 🦅 – RISC-V Educational Emulator

Falcon ASM is an educational RISC-V emulator designed to simplify and visualize RISC-V instruction set architecture (ISA) concepts clearly and interactively. It provides students and enthusiasts with an accessible tool for learning fundamental computer architecture principles such as registers, memory management, and the fetch-decode-execute cycle.

## 🚩 Purpose

- Provide clarity and simplicity in understanding RISC-V instructions.
- Offer interactive visualizations for registers, memory, and instruction flow.
- Serve as an intuitive entry-point to assembly language and processor architecture.

---

## 📏 Word Size

```
Word Len = 32 bits
```

---

## 🧮 Arithmetic Instructions (RISC-V Standard)

| Instruction    | Operation          | Description                 |
|----------------|--------------------|-----------------------------|
| ADD rd, rs1, rs2  | rd = rs1 + rs2      | Integer Addition            |
| SUB rd, rs1, rs2  | rd = rs1 - rs2      | Integer Subtraction         |
| MUL rd, rs1, rs2  | rd = rs1 * rs2      | Integer Multiplication      |
| DIV rd, rs1, rs2  | rd = rs1 / rs2      | Integer Division            |
| ADDI rd, rs1, imm | rd = rs1 + imm      | Integer Add Immediate       |

---

## 🔢 Floating-Point Arithmetic (RISC-V Standard)

| Instruction      | Operation           | Description                 |
|------------------|---------------------|-----------------------------|
| FADD.S fd, fs1, fs2 | fd = fs1 + fs2      | Float Addition (Single)     |
| FSUB.S fd, fs1, fs2 | fd = fs1 - fs2      | Float Subtraction (Single)  |
| FMUL.S fd, fs1, fs2 | fd = fs1 * fs2      | Float Multiplication (Single)|
| FDIV.S fd, fs1, fs2 | fd = fs1 / fs2      | Float Division (Single)     |
| FCVT.S.W fd, rs     | fd = (float) rs     | Integer to Float Conversion |
| FCVT.W.S rd, fs     | rd = (int) fs       | Float to Integer Conversion |

---

## 🧠 RISC-V Registers

- **Temporary Registers:** `t0 – t6` (x5–x7, x28–x31)
- **Saved Registers:** `s0 – s11` (x8–x9, x18–x27)
- **Argument Registers:** `a0 – a7` (x10–x17)
- **Float Registers:** `f0 – f31`
- **Special Registers:** 
  - `zero` (x0, constant 0)
  - `ra` (x1, return address)
  - `sp` (x2, stack pointer)
  - `gp` (x3, global pointer)
  - `tp` (x4, thread pointer)
  - `pc` (program counter)

---

## 📦 Data Sizes

```asm
BYTE        ; 8 bits
HALF WORD   ; 16 bits
WORD        ; 32 bits
```

---

## 📈 Memory Model

Memory grows upwards, consistent with typical RISC-V convention.

**Example of stack operations:**

```asm
addi sp, sp, -8      ; Allocate 8 bytes on stack
sd ra, 0(sp)         ; Save return address
ld ra, 0(sp)         ; Load return address
addi sp, sp, 8       ; Deallocate stack space
```

---

## 🚀 Future Capabilities

- Integrated Code Editor and Assembler
- Visualized Register and Memory State
- Animated Fetch-Decode-Execute Cycle

---

## 📑 Example Program

```asm
.data
value:  .word 10
array:  .word 1, 2, 3, 4
message: .ascii "Hello, Falcon RISC-V!"

.text
la t0, value           # t0 = &value
lw t1, 0(t0)           # t1 = mem[t0]
add t2, t1, t1         # t2 = t1 + t1
ecall                  # System call (halt in emulator)
```


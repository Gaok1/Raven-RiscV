<img src="https://github.com/user-attachments/assets/00e072c9-edb8-4e00-8505-079a7d01152d" alt="FALCON ASM" width="500"/>


# FALCON ASM
 Falcon ASM is a custom-designed low-level assembly language built for precision, speed, and educational clarity. Paired with its own virtual hardware simulator (Falcon), it provides a clean, consistent platform for learning and experimenting with computer architecture, memory models, and instruction-level execution.

# 🦅 Falcon ASM – Structure and Instruction Set Definition



---

## 🧠 Memory Growth Policy

In Falcon ASM, memory always grows upward:  
→ Higher addresses represent newer regions in memory.

This applies to both general memory access and the stack.

**Example of access with offset:**

```asm
LW   R1, 4(R2)      ; R1 = mem[R2 + 4]
```

**Stack behavior (also upward):**

```asm
PUSH: mem[SP] = value; SP = SP + size  
POP:  SP = SP - size; value = mem[SP]
```

---

## 📏 Word Size

```asm
#WORD = 64 BYTES
```

---

## 📦 Data Types

```asm
DATA_SET : 
 - QWORD   ; 1 BYTE
 - HWORD   ; 2 BYTES
 - WORD    ; 4 BYTES
 - DWORD   ; 8 BYTES
```

---

## ➕ Arithmetic Instructions (Integers)

```asm
ADD  R1, R2, R3     ; R1 = R2 + R3

SUB  R1, R2, R3     ; Subtraction

MUL  R1, R2, R3     ; Multiplication

DIV  R1, R2, R3     ; Division

MOV  R1, R2         ; R1 = R2
```

---

## 🔢 Arithmetic Instructions – Float

```asm
ADDF  F1,  F2, F3       ; F1 = F2 + F3
SUBF  F1,  F2, F3
MULF  F1,  F2, F3
DIVF  F1,  F2, F3
```

---

## 🔁 Control Flow

```asm
JMP    LABEL            ; Unconditional jump
JNZ    R1, LABEL        ; Jump if R1 ≠ 0
JZ     R1, LABEL        ; Jump if R1 == 0
JGT    R1, R2, LABEL    ; Jump if R1 > R2
JLT    R1, R2, LABEL    ; Jump if R1 < R2
JGE    R1, R2, LABEL    ; Jump if R1 ≥ R2
JLE    R1, R2, LABEL    ; Jump if R1 ≤ R2
BEGIN                   ; Start of loop block
END                     ; End of loop block
HALT                    ; End of program
```

---

## 💾 Memory Access (Load/Store)

```asm
; Load
LQW    R1, offset(R2)   ; Load QWORD  → R1
LHW    R1, offset(R2)   ; Load HWORD  → R1
LW     R1, offset(R2)   ; Load WORD   → R1
LDW    R1, offset(R2)   ; Load DWORD  → R1
LA     R1, LABEL        ; Load address of LABEL → R1

; Store 
SQW    offset(R1), R2   ; Store QWORD  → mem[R1 + offset] = R2
SHW    offset(R1), R2   ; Store HWORD  → mem[R1 + offset] = R2
SW     offset(R1), R2   ; Store WORD   → mem[R1 + offset] = R2
SDW    offset(R1), R2   ; Store DWORD  → mem[R1 + offset] = R2
```

---

## 🧮 Pointer Arithmetic

```asm
; Aritmética via Pointer

PTADD offset(R1), R2, R3   ; Store QWORD  → mem[R1 + offset] = R2 + R3

PTSUB offset(R1), R2, R3   ; Store QWORD  → mem[R1 + offset] = R2 - R3

PTMUL offset(R1), R2, R3   ; Store QWORD  → mem[R1 + offset] = R2 * R3

PTDIV offset(R1), R2, R3   ; Store QWORD  → mem[R1 + offset] = R2 / R3
```

---



## 💾 Load/Store for Float

```asm
FLW    F1, offset(R2)   ; Load float  → F1
FSD    F1, offset(R2)   ; Store float ← F1
```

---

## 🔁 Integer/Float Conversion

```asm
ITOF   F1, R1           ; F1 = (float) R1
FTOI   R1, F1           ; R1 = (int) F1
```

---

## 📥 Stack

```asm
; PUSH
PUSH.Q R1              ; Push 1 byte
PUSH.W R1              ; Push 4 bytes
PUSH.D R1              ; Push 8 bytes

; POP
POP.Q  R1              ; Pop 1 byte
POP.W  R1              ; Pop 4 bytes
POP.D  R1              ; Pop 8 bytes

; PEEK
PEEK.Q R1              ; Read 1 byte from stack top
PEEK.W R1              ; Read 4 bytes from stack top
PEEK.D R1              ; Read 8 bytes from stack top
```

---

## 🧠 Registers – Falcon ASM

- **Temporaries:** `T0`, – `T4`
- **Saved:** `S0`, – `S4`
- **Arguments:** `A0`, – `A4`
- **Float:** `F0` – `F7`
- **Control:** `SP`, `PC`, `RA`, `R0` (zero constant)

---

## 🗂️ Section Directives

```asm
.data
value:  .word 10
array:  .word 1, 2, 3, 4
texto:  .ascii "Olá, Falcon"

.text
LA  T0, value         ; T0 = &value
LW  T1, 0(T0)         ; T1 = mem[T0]
PTADD  0(T0), T1, T1  ; value = value + value
HALT
```

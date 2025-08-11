# Falcon ASM – Instruction Encoding & Opcode Reference (RISC‑V Based)

Falcon ASM is an educational RISC‑V emulator designed for learning computer architecture and assembly language. This document lists the opcodes used in Falcon ASM, the instructions they represent, and the bit allocations for each instruction format.

> **Note:** All Falcon ASM instructions are encoded in a fixed 32-bit format (4 bytes). For each instruction type, the fields are allocated as follows:

---

## General 32-bit Instruction Format (R‑type Example)

| Field    | Bit Positions   | Description                                                    |
|----------|-----------------|----------------------------------------------------------------|
| opcode   | [6:0]           | Primary opcode (7 bits) identifying the instruction family.    |
| rd       | [11:7]          | Destination register (5 bits).                                 |
| funct3   | [14:12]         | Secondary opcode field (3 bits) – specifies the operation.       |
| rs1      | [19:15]         | First source register (5 bits).                                |
| rs2      | [24:20]         | Second source register (5 bits) (for R‑type instructions).        |
| funct7   | [31:25]         | Additional opcode field (7 bits) (used to differentiate similar instructions, e.g., ADD vs. SUB). |

*Note:* Other instruction formats (I, S, B, U, J) rearrange or reinterpret these fields, but the primary opcode always occupies bits [6:0].

---

## 1. Opcodes & Associated Instruction Types

### **R‑type (Arithmetic / Logic Register‑Register)**
- **Opcode:** `0x33`
- **Bit Allocation:** opcode in bits [6:0]
- **Instructions:** 
  - `ADD`, `SUB`, `SLL`, `SLT`, `SLTU`, `XOR`, `SRL`, `SRA`, `OR`, `AND`
  - *Extensions (RV32M):* `MUL`, `MULH`, `MULHSU`, `MULHU`
  
*Example of bit positions (R‑type):*
- `funct7`: bits [31:25]
- `rs2`: bits [24:20]
- `rs1`: bits [19:15]
- `funct3`: bits [14:12]
- `rd`: bits [11:7]
- `opcode`: bits [6:0] = 0x33

---

### **I‑type (Immediate Arithmetic / Logical Immediate)**
- **Opcode:** `0x13`
- **Instructions:**
  - `ADDI`, `SLLI`, `SLTI`, `SLTIU`, `XORI`, `SRLI`, `SRAI`, `ORI`, `ANDI`
- **Format:**  
  | immediate (12 bits) | rs1 (5 bits) | funct3 (3 bits) | rd (5 bits) | opcode (7 bits) |
  
---

### **I‑type (Load Instructions)**
- **Opcode:** `0x03`
- **Instructions:** 
  - `LB` (Load Byte), `LH` (Load Halfword), `LW` (Load Word),
  - `LBU` (Load Byte Unsigned), `LHU` (Load Halfword Unsigned)
- **Format:**  
  | immediate (12 bits) | rs1 (5 bits) | funct3 (3 bits) | rd (5 bits) | opcode (7 bits) |

---

### **I‑type (JALR)**
- **Opcode:** `0x67`
- **Instruction:** `JALR` (Jump and Link Register)
- **Format:**  
  | immediate (12 bits) | rs1 (5 bits) | funct3 (3 bits) | rd (5 bits) | opcode (7 bits) |
- **Note:** The funct3 field is always 0 for JALR.

---

### **S‑type (Store Instructions)**
- **Opcode:** `0x23`
- **Instructions:** 
  - `SB` (Store Byte), `SH` (Store Halfword), `SW` (Store Word)
- **Format:**  
  | immediate[11:5] (7 bits) | rs2 (5 bits) | rs1 (5 bits) | funct3 (3 bits) | immediate[4:0] (5 bits) | opcode (7 bits) |
  
---

### **B‑type (Branch Instructions)**
- **Opcode:** `0x63`
- **Instructions:** 
  - `BEQ`, `BNE`, `BLT`, `BGE`, `BLTU`, `BGEU`
- **Format:**  
  | immediate[12|10:5] (7 bits) | rs2 (5 bits) | rs1 (5 bits) | funct3 (3 bits) | immediate[4:1|11] (5 bits) | opcode (7 bits) |

---

### **U‑type (Upper Immediate Instructions)**
- **Opcodes:**
  - `LUI`: `0x37`
  - `AUIPC`: `0x17`
- **Instructions:**  
  - `LUI` (Load Upper Immediate)
  - `AUIPC` (Add Upper Immediate to PC)
- **Format:**  
  | immediate (20 bits) | rd (5 bits) | opcode (7 bits) |

---

### **J‑type (Jump and Link)**
- **Opcode:** `0x6F`
- **Instruction:** `JAL`
- **Format:**  
  | immediate (20 bits) | rd (5 bits) | opcode (7 bits) |

---

### **MISC_MEM (Fence)**
- **Opcode:** `0x0F`
- **Instructions:** 
  - `FENCE`, `FENCE.I`

---

### **SYSTEM**
- **Opcode:** `0x73`
- **Instructions:** 
  - `ECALL`, `EBREAK`, and CSR instructions
- **Format:** Variável, mas o campo `funct3` para `ECALL`/`EBREAK` é 0.

---

## 2. FUNCT3 and FUNCT7 Fields (For R‑type and I‑type Instructions)

### **FUNCT3 (3 bits, bits [14:12]):**

- For R‑type Arithmetic:
  - `0x0`: Used for ADD and SUB.
  - `0x1`: SLL (Shift Left Logical)
  - `0x2`: SLT (Set Less Than)
  - `0x3`: SLTU (Set Less Than Unsigned)
  - `0x4`: XOR
  - `0x5`: SRL/SRA (Shift Right Logical/Arithmetic)
  - `0x6`: OR
  - `0x7`: AND

- For Immediate Arithmetic (I‑type OP_IMM):
  - Same encoding as above applies.

- For Loads:
  - `0x0`: LB
  - `0x1`: LH
  - `0x2`: LW
  - `0x4`: LBU
  - `0x5`: LHU

- For Stores:
  - `0x0`: SB
  - `0x1`: SH
  - `0x2`: SW

- For Branches:
  - `0x0`: BEQ
  - `0x1`: BNE
  - `0x4`: BLT
  - `0x5`: BGE
  - `0x6`: BLTU
  - `0x7`: BGEU

- For JALR:
  - Always `0x0`.

### **FUNCT7 (7 bits, bits [31:25]):**

- For R‑type Arithmetic:
  - `0x00`: For ADD, SLL, SRL.
  - `0x20`: For SUB, SRA.
  - `0x01`: Used for MUL and other RV32M extensions.

---

## 3. Example: Encoding an R‑type Instruction (ADD)

Consider the instruction:

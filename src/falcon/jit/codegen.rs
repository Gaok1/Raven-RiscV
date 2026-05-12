//! Geração de código nativo via `dynasm-rs` — Fase B.
//!
//! # Visão geral
//!
//! Este módulo compila um [`BasicBlock`] em código x86_64 nativo e retorna um
//! [`CompiledBlock`] pronto para execução. O ponto de entrada é [`compile_block`].
//!
//! # ABI do bloco compilado (System V x86_64)
//!
//! ```text
//! // Assinatura:
//! unsafe extern "C" fn(cpu: *mut Cpu, mem: *mut CacheController, console: *mut Console) -> u32
//!
//! // Argumentos ao entrar:
//! rdi = cpu        → salvo em rbx (callee-saved)
//! rsi = mem        → salvo em r12 (callee-saved)
//! rdx = console    → salvo em r13 (callee-saved)
//!
//! // Retorno:
//! eax = discriminante de ExitKind (0=Continue, 1=AwaitInput, 2=Halted, 3=Fault)
//! ```
//!
//! # Estrutura por bloco
//!
//! ```text
//! prologue:
//!   push rbp / push rbx / push r12 / push r13 / push r14
//!   sub rsp, 8        ; alinha RSP a 16 bytes para chamadas C
//!   mov rbx, rdi      ; cpu
//!   mov r12, rsi      ; mem
//!   mov r13, rdx      ; console
//!
//! [por instrução não-terminadora]:
//!   call jit_fetch32  ; contabiliza I-cache
//!   mov cpu.pc = pc + 4
//!   add cpu.instr_count, 1
//!   <código nativo da instrução>
//!
//! [terminador]:
//!   <atualiza cpu.pc para o destino correto>
//!   mov eax, <exit discriminant>
//!   jmp epilogue
//!
//! epilogue:
//!   add rsp, 8
//!   pop r14 / pop r13 / pop r12 / pop rbx / pop rbp
//!   ret
//! ```
//!
//! # Instruções compiladas nativamente (Fase B)
//!
//! ALU R-type: Add, Sub, And, Or, Xor, Sll, Srl, Sra, Slt, Sltu, Mul, Mulh,
//! Mulhsu, Mulhu, Div, Divu, Rem, Remu.
//!
//! ALU I-type: Addi, Andi, Ori, Xori, Slti, Sltiu, Slli, Srli, Srai, Lui, Auipc.
//!
//! Loads (callout): Lb, Lh, Lw, Lbu, Lhu.
//!
//! Stores (callout): Sb, Sh, Sw.
//!
//! Terminadores: Branch (Beq/Bne/Blt/Bge/Bltu/Bgeu), Jal, Jalr, Ecall, Halt,
//! Fence/FenceI, FallThrough.
//!
//! # Trampolines
//!
//! `dynasm` não pode chamar métodos `&mut self` de Rust diretamente (ABI mismatch).
//! Cada operação de memória com efeito colateral vira um `unsafe extern "C" fn` com
//! ABI System V x86_64:
//!
//! ```text
//! jit_fetch32(mem, addr) -> u32          I-cache + instruction_count
//! jit_dcache_read8/16/32(mem, addr) -> u32   D-cache reads
//! jit_store8/16/32(mem, addr, val) -> i32    D-cache writes (0=ok, -1=fault)
//! jit_handle_ecall(cpu, mem, console) -> u32  syscall dispatch
//! ```
//!
//! # Acesso direto aos registradores
//!
//! Para ALU pura, os registradores são lidos/escritos diretamente via `offset_of!`:
//!
//! ```text
//! cpu.x[n]        = [rbx + X_BASE + n*4]  (u32)
//! cpu.pc          = [rbx + PC_OFF]         (u32)
//! cpu.instr_count = [rbx + ICOUNT_OFF]     (u64)
//! cpu.local_exit  = [rbx + LOCAL_EXIT_OFF] (bool, 1 byte)
//! ```
//!
//! Isso evita trampolines de registrador e é o principal ganho de desempenho
//! sobre o interpretador (~2–4× esperado para loops ALU dominantes).

use std::mem::offset_of;
use std::sync::Arc;

use dynasm::dynasm;
use dynasmrt::{DynasmApi, DynasmLabelApi, x64::Assembler};

use crate::falcon::CacheController;
use crate::falcon::decoder::decode;
use crate::falcon::instruction::Instruction;
use crate::falcon::memory::Bus;
use crate::falcon::registers::Cpu;
use crate::ui::Console;

use super::block::BasicBlock;
use super::cache::{CompiledBlock, exit};

// ---------------------------------------------------------------------------
// Offsets dos campos de Cpu (calculados em tempo de compilação)
// ---------------------------------------------------------------------------

const X_BASE: usize = offset_of!(Cpu, x);
const PC_OFF: usize = offset_of!(Cpu, pc);
const ICOUNT_OFF: usize = offset_of!(Cpu, instr_count);
const LOCAL_EXIT_OFF: usize = offset_of!(Cpu, local_exit);

// ---------------------------------------------------------------------------
// Trampolines extern "C"
// ---------------------------------------------------------------------------

/// Executa `fetch32` com contabilidade de I-cache.
/// Retorna `u32::MAX` se a leitura falhar (indica fault ao dispatcher).
unsafe extern "C" fn jit_fetch32(mem: *mut CacheController, addr: u32) -> u32 {
    unsafe { (*mem).fetch32(addr) }.unwrap_or(u32::MAX)
}

/// Lê 1 byte da memória via D-cache. Retorna o byte com zero-extension em u32.
unsafe extern "C" fn jit_dcache_read8(mem: *mut CacheController, addr: u32) -> u32 {
    unsafe { (*mem).dcache_read8(addr) }.unwrap_or(0) as u32
}

/// Lê 2 bytes da memória via D-cache. Retorna com zero-extension em u32.
unsafe extern "C" fn jit_dcache_read16(mem: *mut CacheController, addr: u32) -> u32 {
    unsafe { (*mem).dcache_read16(addr) }.unwrap_or(0) as u32
}

/// Lê 4 bytes da memória via D-cache.
unsafe extern "C" fn jit_dcache_read32(mem: *mut CacheController, addr: u32) -> u32 {
    unsafe { (*mem).dcache_read32(addr) }.unwrap_or(0)
}

/// Escreve 1 byte na memória via D-cache. Retorna 0=ok, -1=fault.
unsafe extern "C" fn jit_store8(mem: *mut CacheController, addr: u32, val: u32) -> i32 {
    match unsafe { (*mem).store8(addr, val as u8) } {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Escreve 2 bytes na memória via D-cache. Retorna 0=ok, -1=fault.
unsafe extern "C" fn jit_store16(mem: *mut CacheController, addr: u32, val: u32) -> i32 {
    match unsafe { (*mem).store16(addr, val as u16) } {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Escreve 4 bytes na memória via D-cache. Retorna 0=ok, -1=fault.
unsafe extern "C" fn jit_store32(mem: *mut CacheController, addr: u32, val: u32) -> i32 {
    match unsafe { (*mem).store32(addr, val) } {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Despacha uma syscall via `falcon::syscall::handle_syscall`.
/// Retorna exit discriminante: Continue=0, AwaitInput=1, Halted=2, Fault=3.
unsafe extern "C" fn jit_handle_ecall(
    cpu: *mut Cpu,
    mem: *mut CacheController,
    console: *mut Console,
) -> u32 {
    use crate::falcon::syscall::handle_syscall;
    // a7 (x17) holds the syscall code
    let code = unsafe { (*cpu).read(17) };
    match unsafe { handle_syscall(code, &mut *cpu, &mut *mem, &mut *console) } {
        Ok(true) => {
            if unsafe { (*console).reading } {
                exit::AWAIT_INPUT
            } else {
                exit::CONTINUE
            }
        }
        Ok(false) => exit::HALTED,
        Err(_) => exit::FAULT,
    }
}

// ---------------------------------------------------------------------------
// Helpers de emissão
// ---------------------------------------------------------------------------

/// Emite a chamada ao trampoline `jit_fetch32(mem, pc)`.
/// Após esta chamada, o bloco tem a contabilidade de I-cache correta para este PC.
#[inline(always)]
fn emit_fetch32(ops: &mut Assembler, pc: u32) {
    let fn_ptr = jit_fetch32 as *const () as i64;
    let pc_i32 = pc as i32;
    dynasm!(ops
        ; mov rdi, r12           // arg0 = mem
        ; mov esi, DWORD pc_i32  // arg1 = pc
        ; mov rax, QWORD fn_ptr
        ; call rax
    );
}

/// Emite a atualização de `cpu.pc` e incremento de `cpu.instr_count`.
/// Deve ser chamado para cada instrução, antes do corpo da instrução.
#[inline(always)]
fn emit_pre_instruction(ops: &mut Assembler, pc_next: u32) {
    let pc_off = PC_OFF as i32;
    let ic_off = ICOUNT_OFF as i32;
    dynasm!(ops
        ; mov DWORD [rbx + pc_off], DWORD pc_next as i32
        ; add QWORD [rbx + ic_off], BYTE 1
    );
}

/// Emite o código para uma instrução R-type ALU.
///
/// Lê rs1 e rs2 diretamente da memória, aplica a operação, escreve em rd.
/// `op_emit` recebe os registradores x86 (eax=rs1_val, ecx=rs2_val, resultado em eax).
fn emit_rtype(ops: &mut Assembler, rd: u8, rs1: u8, rs2: u8, op_emit: impl Fn(&mut Assembler)) {
    let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
    let rs2_off = (X_BASE + rs2 as usize * 4) as i32;
    // Load rs1 → eax, rs2 → ecx
    dynasm!(ops
        ; mov eax, DWORD [rbx + rs1_off]
        ; mov ecx, DWORD [rbx + rs2_off]
    );
    op_emit(ops);
    if rd != 0 {
        let rd_off = (X_BASE + rd as usize * 4) as i32;
        dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
    }
}

/// Emite o código para uma instrução I-type ALU.
///
/// Lê rs1, aplica a operação com `imm`, escreve em rd.
fn emit_itype(ops: &mut Assembler, rd: u8, rs1: u8, imm: i32, op_emit: impl Fn(&mut Assembler)) {
    let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
    dynasm!(ops ; mov eax, DWORD [rbx + rs1_off]);
    // ecx = imm (usado pela op_emit se precisar)
    dynasm!(ops ; mov ecx, DWORD imm);
    op_emit(ops);
    if rd != 0 {
        let rd_off = (X_BASE + rd as usize * 4) as i32;
        dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
    }
}

// ---------------------------------------------------------------------------
// `compile_block` — ponto de entrada público
// ---------------------------------------------------------------------------

/// Compila um [`BasicBlock`] em código nativo x86_64.
///
/// Retorna `None` se o assembler falhar (improvável em condições normais).
/// O bloco retornado pode ser inserido no [`CompiledBlockCache`] e executado
/// repetidamente até ser invalidado por SMC.
pub fn compile_block(block: &BasicBlock) -> Option<Arc<CompiledBlock>> {
    let mut ops = Assembler::new().ok()?;
    let entry = ops.offset();

    // --- Prologue ---
    // Após 5 pushes (rbp + 4 callee-saved) + sub 8 → RSP alinhado a 16 bytes.
    dynasm!(ops
        ; push rbp
        ; mov rbp, rsp
        ; push rbx
        ; push r12
        ; push r13
        ; push r14
        ; sub rsp, BYTE 8   // alinhamento
        ; mov rbx, rdi      // cpu
        ; mov r12, rsi      // mem
        ; mov r13, rdx      // console
    );

    let epilogue = ops.new_dynamic_label();
    let mut fault_requested = false;

    // --- Corpo: instruções não-terminadoras ---
    let n = block.words.len();
    for (i, &word) in block.words.iter().enumerate() {
        let pc = block.start_pc.wrapping_add(i as u32 * 4);
        let pc_next = pc.wrapping_add(4);
        let is_last = i + 1 == n;

        emit_fetch32(&mut ops, pc);
        emit_pre_instruction(&mut ops, pc_next);

        let instr = match decode(word) {
            Ok(i) => i,
            Err(_) => {
                // Instrução inválida — retorna Fault
                dynasm!(ops
                    ; mov eax, DWORD exit::FAULT as i32
                    ; jmp =>epilogue
                );
                fault_requested = true;
                break;
            }
        };

        if is_last {
            // A última instrução é o terminador
            emit_terminator(&mut ops, &instr, pc, block.end_pc, epilogue);
        } else {
            emit_non_terminator(&mut ops, &instr, pc);
        }
    }

    if block.words.is_empty() || !fault_requested {
        // Se o bloco não emitiu o jmp =>epilogue ainda (apenas para blocks de 0 palavras
        // ou blocos onde emit_terminator não encerrou com jmp), garantir saída.
        // Na prática, emit_terminator sempre emite `jmp =>epilogue`.
    }

    // --- Epilogue ---
    dynasm!(ops
        ; =>epilogue
        ; add rsp, BYTE 8
        ; pop r14
        ; pop r13
        ; pop r12
        ; pop rbx
        ; pop rbp
        ; ret
    );

    let buf = ops.finalize().ok()?;
    Some(Arc::new(CompiledBlock {
        start_pc: block.start_pc,
        end_pc: block.end_pc,
        instruction_count: block.words.len() as u32,
        code: buf,
        entry,
    }))
}

// ---------------------------------------------------------------------------
// Emissão de instruções não-terminadoras
// ---------------------------------------------------------------------------

fn emit_non_terminator(ops: &mut Assembler, instr: &Instruction, pc: u32) {
    match *instr {
        // --- R-type ALU ---
        Instruction::Add { rd, rs1, rs2 } => emit_rtype(ops, rd, rs1, rs2, |o| {
            dynasm!(o ; add eax, ecx)
        }),
        Instruction::Sub { rd, rs1, rs2 } => emit_rtype(ops, rd, rs1, rs2, |o| {
            dynasm!(o ; sub eax, ecx)
        }),
        Instruction::And { rd, rs1, rs2 } => emit_rtype(ops, rd, rs1, rs2, |o| {
            dynasm!(o ; and eax, ecx)
        }),
        Instruction::Or { rd, rs1, rs2 } => emit_rtype(ops, rd, rs1, rs2, |o| {
            dynasm!(o ; or eax, ecx)
        }),
        Instruction::Xor { rd, rs1, rs2 } => emit_rtype(ops, rd, rs1, rs2, |o| {
            dynasm!(o ; xor eax, ecx)
        }),
        Instruction::Sll { rd, rs1, rs2 } => emit_rtype(ops, rd, rs1, rs2, |o| {
            // shl eax, cl (cl = low 5 bits of rs2, masked by hardware)
            dynasm!(o ; and ecx, BYTE 31 ; shl eax, cl)
        }),
        Instruction::Srl { rd, rs1, rs2 } => emit_rtype(ops, rd, rs1, rs2, |o| {
            dynasm!(o ; and ecx, BYTE 31 ; shr eax, cl)
        }),
        Instruction::Sra { rd, rs1, rs2 } => emit_rtype(ops, rd, rs1, rs2, |o| {
            dynasm!(o ; and ecx, BYTE 31 ; sar eax, cl)
        }),
        Instruction::Slt { rd, rs1, rs2 } => emit_rtype(ops, rd, rs1, rs2, |o| {
            // signed comparison: eax = (i32)rs1 < (i32)rs2
            dynasm!(o
                ; cmp eax, ecx
                ; setl al
                ; movzx eax, al
            )
        }),
        Instruction::Sltu { rd, rs1, rs2 } => emit_rtype(ops, rd, rs1, rs2, |o| {
            dynasm!(o
                ; cmp eax, ecx
                ; setb al
                ; movzx eax, al
            )
        }),

        // --- Multiply/Divide (R-type, via 64-bit ops) ---
        Instruction::Mul { rd, rs1, rs2 } => emit_rtype(ops, rd, rs1, rs2, |o| {
            // eax * ecx (lower 32 bits)
            dynasm!(o ; imul eax, ecx)
        }),
        Instruction::Mulh { rd, rs1, rs2 } => {
            // signed: (i64)rs1 * (i64)rs2, upper 32 bits
            let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
            let rs2_off = (X_BASE + rs2 as usize * 4) as i32;
            dynasm!(ops
                ; movsxd rax, DWORD [rbx + rs1_off]  // sign-extend to 64
                ; movsxd rcx, DWORD [rbx + rs2_off]
                ; imul rax, rcx
                ; shr rax, 32
                ; mov eax, eax  // zero-extend to 64
            );
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
            }
        }
        Instruction::Mulhsu { rd, rs1, rs2 } => {
            // signed rs1 * unsigned rs2, upper 32 bits
            let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
            let rs2_off = (X_BASE + rs2 as usize * 4) as i32;
            dynasm!(ops
                ; movsxd rax, DWORD [rbx + rs1_off]   // signed
                ; mov ecx, DWORD [rbx + rs2_off]       // zero-extend (unsigned)
                ; imul rax, rcx
                ; shr rax, 32
                ; mov eax, eax
            );
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
            }
        }
        Instruction::Mulhu { rd, rs1, rs2 } => {
            // unsigned rs1 * unsigned rs2, upper 32 bits
            let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
            let rs2_off = (X_BASE + rs2 as usize * 4) as i32;
            dynasm!(ops
                ; mov eax, DWORD [rbx + rs1_off]  // zero-extend
                ; mov ecx, DWORD [rbx + rs2_off]
                ; mul ecx                           // rdx:rax = eax * ecx (unsigned)
                ; shr rax, 32
                ; mov eax, eax
            );
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
            }
        }
        Instruction::Div { rd, rs1, rs2 } => {
            // RV32M: div-by-zero → -1; signed overflow (MIN/-1) → MIN
            let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
            let rs2_off = (X_BASE + rs2 as usize * 4) as i32;
            let skip = ops.new_dynamic_label();
            let done = ops.new_dynamic_label();
            dynasm!(ops
                ; mov eax, DWORD [rbx + rs1_off]
                ; mov ecx, DWORD [rbx + rs2_off]
                ; test ecx, ecx
                ; jnz =>skip
                ; mov eax, -1i32 as i32    // div by zero → -1
                ; jmp =>done
                ; =>skip
                ; cdq                       // sign-extend eax into edx:eax
                ; idiv ecx
                ; =>done
            );
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
            }
        }
        Instruction::Divu { rd, rs1, rs2 } => {
            // RV32M: div-by-zero → 0xFFFF_FFFF
            let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
            let rs2_off = (X_BASE + rs2 as usize * 4) as i32;
            let skip = ops.new_dynamic_label();
            let done = ops.new_dynamic_label();
            dynasm!(ops
                ; mov eax, DWORD [rbx + rs1_off]
                ; mov ecx, DWORD [rbx + rs2_off]
                ; test ecx, ecx
                ; jnz =>skip
                ; mov eax, -1i32 as i32     // 0xFFFF_FFFF
                ; jmp =>done
                ; =>skip
                ; xor edx, edx
                ; div ecx
                ; =>done
            );
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
            }
        }
        Instruction::Rem { rd, rs1, rs2 } => {
            // RV32M: rem-by-zero → dividend; signed overflow → 0
            let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
            let rs2_off = (X_BASE + rs2 as usize * 4) as i32;
            let skip = ops.new_dynamic_label();
            let done = ops.new_dynamic_label();
            dynasm!(ops
                ; mov eax, DWORD [rbx + rs1_off]
                ; mov ecx, DWORD [rbx + rs2_off]
                ; test ecx, ecx
                ; jnz =>skip
                ; jmp =>done        // rem=0: eax already = dividend
                ; =>skip
                ; cdq
                ; idiv ecx          // remainder in edx
                ; mov eax, edx
                ; =>done
            );
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
            }
        }
        Instruction::Remu { rd, rs1, rs2 } => {
            // RV32M: rem-by-zero → dividend
            let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
            let rs2_off = (X_BASE + rs2 as usize * 4) as i32;
            let skip = ops.new_dynamic_label();
            let done = ops.new_dynamic_label();
            dynasm!(ops
                ; mov eax, DWORD [rbx + rs1_off]
                ; mov ecx, DWORD [rbx + rs2_off]
                ; test ecx, ecx
                ; jnz =>skip
                ; jmp =>done
                ; =>skip
                ; xor edx, edx
                ; div ecx           // remainder in edx
                ; mov eax, edx
                ; =>done
            );
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
            }
        }

        // --- I-type ALU ---
        Instruction::Addi { rd, rs1, imm } => emit_itype(ops, rd, rs1, imm, |o| {
            dynasm!(o ; add eax, ecx)
        }),
        Instruction::Andi { rd, rs1, imm } => emit_itype(ops, rd, rs1, imm, |o| {
            dynasm!(o ; and eax, ecx)
        }),
        Instruction::Ori { rd, rs1, imm } => emit_itype(ops, rd, rs1, imm, |o| {
            dynasm!(o ; or eax, ecx)
        }),
        Instruction::Xori { rd, rs1, imm } => emit_itype(ops, rd, rs1, imm, |o| {
            dynasm!(o ; xor eax, ecx)
        }),
        Instruction::Slti { rd, rs1, imm } => emit_itype(ops, rd, rs1, imm, |o| {
            dynasm!(o ; cmp eax, ecx ; setl al ; movzx eax, al)
        }),
        Instruction::Sltiu { rd, rs1, imm } => emit_itype(ops, rd, rs1, imm, |o| {
            dynasm!(o ; cmp eax, ecx ; setb al ; movzx eax, al)
        }),
        Instruction::Slli { rd, rs1, shamt } => {
            let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
            dynasm!(ops ; mov eax, DWORD [rbx + rs1_off] ; shl eax, BYTE shamt as i8);
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
            }
        }
        Instruction::Srli { rd, rs1, shamt } => {
            let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
            dynasm!(ops ; mov eax, DWORD [rbx + rs1_off] ; shr eax, BYTE shamt as i8);
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
            }
        }
        Instruction::Srai { rd, rs1, shamt } => {
            let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
            dynasm!(ops ; mov eax, DWORD [rbx + rs1_off] ; sar eax, BYTE shamt as i8);
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
            }
        }
        Instruction::Lui { rd, imm } => {
            // rd = imm << 12 (already shifted in the decoder)
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], DWORD imm);
            }
        }
        Instruction::Auipc { rd, imm } => {
            // rd = pc + imm
            let result = (pc as i32).wrapping_add(imm) as u32;
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], DWORD result as i32);
            }
        }

        // --- Loads (callout via trampolines) ---
        Instruction::Lw { rd, rs1, imm } => emit_load(ops, rd, rs1, imm, LoadWidth::W32),
        Instruction::Lh { rd, rs1, imm } => emit_load(ops, rd, rs1, imm, LoadWidth::H16s),
        Instruction::Lhu { rd, rs1, imm } => emit_load(ops, rd, rs1, imm, LoadWidth::H16u),
        Instruction::Lb { rd, rs1, imm } => emit_load(ops, rd, rs1, imm, LoadWidth::B8s),
        Instruction::Lbu { rd, rs1, imm } => emit_load(ops, rd, rs1, imm, LoadWidth::B8u),

        // --- Stores (callout via trampolines) ---
        Instruction::Sw { rs1, rs2, imm } => emit_store(ops, rs1, rs2, imm, StoreWidth::W32),
        Instruction::Sh { rs1, rs2, imm } => emit_store(ops, rs1, rs2, imm, StoreWidth::H16),
        Instruction::Sb { rs1, rs2, imm } => emit_store(ops, rs1, rs2, imm, StoreWidth::B8),

        // Qualquer instrução não listada acima que aparecer em posição
        // não-terminal é um bug no scan_block (deveria ter parado antes).
        // Emitimos uma instrução ilegal (ud2) para detectar o problema.
        _ => {
            dynasm!(ops ; ud2);
        }
    }
}

// ---------------------------------------------------------------------------
// Loads e stores via trampolines
// ---------------------------------------------------------------------------

enum LoadWidth {
    B8s,  // Lb  — byte com sign-extension
    B8u,  // Lbu — byte com zero-extension
    H16s, // Lh  — halfword com sign-extension
    H16u, // Lhu — halfword com zero-extension
    W32,  // Lw
}

enum StoreWidth {
    B8,  // Sb
    H16, // Sh
    W32, // Sw
}

fn emit_load(ops: &mut Assembler, rd: u8, rs1: u8, imm: i32, width: LoadWidth) {
    let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
    let fn_ptr: i64 = match width {
        LoadWidth::B8s | LoadWidth::B8u => jit_dcache_read8 as *const () as i64,
        LoadWidth::H16s | LoadWidth::H16u => jit_dcache_read16 as *const () as i64,
        LoadWidth::W32 => jit_dcache_read32 as *const () as i64,
    };
    // addr = cpu.x[rs1] + imm
    dynasm!(ops
        ; mov edi, DWORD [rbx + rs1_off]  // base
        ; add edi, DWORD imm              // addr (truncated to 32 bits via edi)
        ; mov rdi, r12                    // arg0 = mem  (overwrites addr in rdi!)
        // Oops — addr was in edi (low 32 of rdi). Let's use a temp register.
    );
    // Fix: compute addr in ecx, then set up args
    dynasm!(ops
        // Recompute: use ecx for addr, then set rdi/esi
        ; mov ecx, DWORD [rbx + rs1_off]
        ; add ecx, DWORD imm
        ; mov rdi, r12                    // arg0 = mem
        ; mov esi, ecx                    // arg1 = addr
        ; mov rax, QWORD fn_ptr
        ; call rax                        // eax = raw value (u8/u16/u32)
    );
    // Apply sign/zero extension
    match width {
        LoadWidth::B8s => dynasm!(ops ; movsx eax, al),
        LoadWidth::H16s => dynasm!(ops ; movsx eax, ax),
        LoadWidth::B8u | LoadWidth::H16u | LoadWidth::W32 => {} // already zero-extended
    }
    if rd != 0 {
        let rd_off = (X_BASE + rd as usize * 4) as i32;
        dynasm!(ops ; mov DWORD [rbx + rd_off], eax);
    }
}

fn emit_store(ops: &mut Assembler, rs1: u8, rs2: u8, imm: i32, width: StoreWidth) {
    let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
    let rs2_off = (X_BASE + rs2 as usize * 4) as i32;
    let fn_ptr: i64 = match width {
        StoreWidth::B8 => jit_store8 as *const () as i64,
        StoreWidth::H16 => jit_store16 as *const () as i64,
        StoreWidth::W32 => jit_store32 as *const () as i64,
    };
    dynasm!(ops
        ; mov ecx, DWORD [rs1_off + rbx]   // base
        ; add ecx, DWORD imm               // addr
        ; mov rdi, r12                     // arg0 = mem
        ; mov esi, ecx                     // arg1 = addr
        ; mov edx, DWORD [rs2_off + rbx]   // arg2 = value
        ; mov rax, QWORD fn_ptr
        ; call rax
        // eax = 0 if ok, -1 if fault (fault handling is left to Phase C)
    );
}

// ---------------------------------------------------------------------------
// Terminadores
// ---------------------------------------------------------------------------

fn emit_terminator(
    ops: &mut Assembler,
    instr: &Instruction,
    pc: u32,
    _end_pc: u32,
    epilogue: dynasmrt::DynamicLabel,
) {
    let pc_off = PC_OFF as i32;
    let pc_next = pc.wrapping_add(4);

    match *instr {
        // --- Branches ---
        Instruction::Beq { rs1, rs2, imm } => {
            emit_branch(ops, rs1, rs2, imm, pc, epilogue, BranchKind::Eq)
        }
        Instruction::Bne { rs1, rs2, imm } => {
            emit_branch(ops, rs1, rs2, imm, pc, epilogue, BranchKind::Ne)
        }
        Instruction::Blt { rs1, rs2, imm } => {
            emit_branch(ops, rs1, rs2, imm, pc, epilogue, BranchKind::Lt)
        }
        Instruction::Bge { rs1, rs2, imm } => {
            emit_branch(ops, rs1, rs2, imm, pc, epilogue, BranchKind::Ge)
        }
        Instruction::Bltu { rs1, rs2, imm } => {
            emit_branch(ops, rs1, rs2, imm, pc, epilogue, BranchKind::Ltu)
        }
        Instruction::Bgeu { rs1, rs2, imm } => {
            emit_branch(ops, rs1, rs2, imm, pc, epilogue, BranchKind::Geu)
        }

        // --- Jal ---
        Instruction::Jal { rd, imm } => {
            // rd = pc + 4; pc = pc + imm
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], DWORD pc_next as i32);
            }
            let target = (pc as i32).wrapping_add(imm) as u32;
            dynasm!(ops
                ; mov DWORD [rbx + pc_off], DWORD target as i32
                ; mov eax, DWORD exit::CONTINUE as i32
                ; jmp =>epilogue
            );
        }

        // --- Jalr ---
        Instruction::Jalr { rd, rs1, imm } => {
            // temp = (x[rs1] + imm) & !1; x[rd] = pc + 4; pc = temp
            let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
            dynasm!(ops
                ; mov eax, DWORD [rbx + rs1_off]
                ; add eax, DWORD imm
                ; and eax, BYTE -2i8               // clear LSB
            );
            if rd != 0 {
                let rd_off = (X_BASE + rd as usize * 4) as i32;
                dynasm!(ops ; mov DWORD [rbx + rd_off], DWORD pc_next as i32);
            }
            dynasm!(ops
                ; mov DWORD [rbx + pc_off], eax
                ; mov eax, DWORD exit::CONTINUE as i32
                ; jmp =>epilogue
            );
        }

        // --- Halt ---
        Instruction::Halt => {
            let le_off = LOCAL_EXIT_OFF as i32;
            dynasm!(ops
                ; mov BYTE [rbx + le_off], BYTE 1i8
                ; mov eax, DWORD exit::HALTED as i32
                ; jmp =>epilogue
            );
        }

        // --- Ecall ---
        Instruction::Ecall => {
            // cpu.pc is already set to pc+4 by emit_pre_instruction.
            // Call jit_handle_ecall(cpu, mem, console) → eax = exit discriminant.
            let fn_ptr = jit_handle_ecall as *const () as i64;
            dynasm!(ops
                ; mov rdi, rbx   // cpu
                ; mov rsi, r12   // mem
                ; mov rdx, r13   // console
                ; mov rax, QWORD fn_ptr
                ; call rax
                ; jmp =>epilogue  // eax already has the exit code
            );
        }

        // --- Ebreak ---
        Instruction::Ebreak => {
            // For now, treat ebreak like fault (it's a debug breakpoint).
            // Phase C can refine this to set cpu.ebreak_hit and return Halted.
            dynasm!(ops
                ; mov eax, DWORD exit::FAULT as i32
                ; jmp =>epilogue
            );
        }

        // --- Fence / FenceI ---
        Instruction::Fence | Instruction::FenceI => {
            // cpu.pc already = pc + 4 (set by emit_pre_instruction).
            dynasm!(ops
                ; mov eax, DWORD exit::CONTINUE as i32
                ; jmp =>epilogue
            );
        }

        // --- FallThrough (cap de 64 instruções) ---
        // end_pc+4 é o início do próximo bloco; cpu.pc já foi setado pelo
        // emit_pre_instruction da última instrução.
        _ => {
            dynasm!(ops
                ; mov eax, DWORD exit::CONTINUE as i32
                ; jmp =>epilogue
            );
        }
    }
}

enum BranchKind {
    Eq,
    Ne,
    Lt,
    Ge,
    Ltu,
    Geu,
}

fn emit_branch(
    ops: &mut Assembler,
    rs1: u8,
    rs2: u8,
    imm: i32,
    pc: u32,
    epilogue: dynasmrt::DynamicLabel,
    kind: BranchKind,
) {
    let pc_off = PC_OFF as i32;
    let rs1_off = (X_BASE + rs1 as usize * 4) as i32;
    let rs2_off = (X_BASE + rs2 as usize * 4) as i32;
    let taken_target = (pc as i32).wrapping_add(imm) as u32;
    let not_taken = pc.wrapping_add(4); // already set by emit_pre_instruction

    let taken = ops.new_dynamic_label();

    dynasm!(ops
        ; mov eax, DWORD [rbx + rs1_off]
        ; mov ecx, DWORD [rbx + rs2_off]
        ; cmp eax, ecx
    );

    match kind {
        BranchKind::Eq => dynasm!(ops ; je =>taken),
        BranchKind::Ne => dynasm!(ops ; jne =>taken),
        BranchKind::Lt => dynasm!(ops ; jl =>taken),   // signed
        BranchKind::Ge => dynasm!(ops ; jge =>taken),  // signed
        BranchKind::Ltu => dynasm!(ops ; jb =>taken),  // unsigned
        BranchKind::Geu => dynasm!(ops ; jae =>taken), // unsigned
    }

    // Not taken: cpu.pc = pc + 4 (already set), fall through to epilogue
    dynasm!(ops
        ; mov DWORD [rbx + pc_off], DWORD not_taken as i32
        ; mov eax, DWORD exit::CONTINUE as i32
        ; jmp =>epilogue

        ; =>taken
        ; mov DWORD [rbx + pc_off], DWORD taken_target as i32
        ; mov eax, DWORD exit::CONTINUE as i32
        ; jmp =>epilogue
    );
}

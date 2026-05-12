# JIT Roadmap

Status atual: **Fase B concluída** (codegen + infraestrutura). `scan_block` implementado e testado (8 testes), `compile_block` emite x86_64 nativo via dynasm-rs 5 para ALU R/I-type completo + loads/stores (trampolines) + todos os terminadores. `CompiledBlockCache` real com `HashMap<u32, Arc<CompiledBlock>>` e `invalidate_range`. Todos os 8 módulos do `src/falcon/jit/` documentados com `//!`. **Pendente:** `HotBackend` em `factory.rs` para conectar o codegen ao loop de execução (`--jit=hot`).

Decisões arquiteturais que continuam valendo nas próximas fases:

- **Codegen:** `dynasm-rs` (gated atrás da cargo feature `jit`). Alvos: `x86_64-linux-{gnu,musl}` e `aarch64-linux-gnu` (os mesmos do `Cross.toml`).
- **Fidelidade de métricas:** callouts fiéis por instrução. Cada instrução compilada continua chamando `mem.fetch32`, `mem.dcache_*` / `mem.store*`, `mem.add_instruction_cycles` e `mem.snapshot_stats`. Speedup esperado ~2–4×; métricas idênticas ao interpretador.
- **Escopo de entrada:** **format-agnostic**. Suporta os 4 formatos do Raven (ELF, FALC, ASM inline, flat binary). Nenhum scanner deve ler `ElfInfo`; o ponto de partida é sempre `cpu.pc` pós-load.
- **Threshold do Hot mode:** **500 entradas** em um PC antes de compilar. O interpretador do Raven é deliberadamente lento para fins didáticos, então 500 já caracteriza um bloco quente.

---

## Fase B — Scaffold de codegen ✓ CONCLUÍDA

Objetivo: provar viabilidade compilando UM basic block de instruções ALU e validando equivalência bit-exata contra o interpretador. Sem políticas de hot/full ainda.

### Dependências

- Adicionar ao `Cargo.toml`:
  ```toml
  [features]
  jit = ["dep:dynasm", "dep:dynasmrt"]
  
  [dependencies]
  dynasm   = { version = "3", optional = true }
  dynasmrt = { version = "3", optional = true }
  ```
- Build path: `cargo build --features jit` compila o codegen; `cargo build` (default) continua sem dependência.

### `src/falcon/jit/block.rs` — detecção de basic block

Atualmente um stub. Implementar `scan_block(mem: &CacheController, start_pc: u32) -> BasicBlock`:

1. Lê words sequencialmente via `mem.peek32(pc)` (não conta no I-cache durante o scan).
2. Decodifica cada palavra via `falcon::decoder::decode`.
3. Para de varrer ao encontrar: `Branch` (B-type), `Jal`, `Jalr`, `Ecall`, `Ebreak`, `Halt`, `Fence`, `FenceI`.
4. Limite de segurança: máximo 64 instruções por bloco (cap para blocos patológicos).
5. Retorna `BasicBlock { start_pc, end_pc, words, terminator }`.

### `src/falcon/jit/cache.rs` — `CompiledBlockCache` real

Substituir o stub:

```rust
pub struct CompiledBlock {
    pub start_pc: u32,
    pub end_pc: u32,
    pub code: ExecutableBuffer,  // dynasmrt::ExecutableBuffer
    pub entry: AssemblyOffset,
}

pub struct CompiledBlockCache {
    blocks: HashMap<u32, Arc<CompiledBlock>>,
}

impl CompiledBlockCache {
    pub fn get(&self, pc: u32) -> Option<Arc<CompiledBlock>>;
    pub fn insert(&mut self, block: Arc<CompiledBlock>);
    pub fn invalidate_range(&mut self, start: u32, end: u32);  // já tem stub
}
```

Range invalidation: `retain(|_, b| b.end_pc <= start || b.start_pc >= end)`.

### `src/falcon/jit/codegen.rs` — emissão dynasm

Estrutura por bloco:

```c
// Pseudo-código do que o JIT emite, por instrução do bloco:

// 1) prólogo do bloco (ABI System V x86_64):
//    rdi = *mut Cpu, rsi = *mut CacheController, rdx = *mut Console
push rbp; mov rbp, rsp
sub rsp, <local_frame>
save callee-saved (rbx, r12-r15) conforme uso

// 2) por instrução:
mov rax, <pc_atual>
mov rsi, [cpu_ptr]
call mem.fetch32_trampoline           // mantém I-cache fiel

<dispatch da instrução>:
  - ALU/IMM (Add, Sub, And, Or, Xor, Sll, Srl, Sra, Slt[u], Slti[u], Addi, Andi, Ori, Xori, Lui, Auipc):
        nativa, lê/escreve direto em cpu.x[i] via offset.
  - Loads (Lb, Lh, Lw, Lbu, Lhu):
        call mem.dcache_read{8,16,32}_trampoline + tratamento de sign-extension
  - Stores (Sb, Sh, Sw):
        call mem.store{8,16,32}_trampoline
  - Mul/Div/Rem (RV32M):  call mul/div_trampoline (nativo seria simples mas mantém callout p/ contabilidade)

mov rdi, cpu_ptr
mov rsi, <cpi_cycles_da_instrucao>
call CacheController::add_instruction_cycles_trampoline
call CacheController::snapshot_stats_trampoline

// 3) terminador:
//    Branch/Jal/Jalr/Ecall/Halt:
//      atualiza cpu.pc apropriadamente
//      mov rax, <ExitInfo discriminant>
//      jmp epilogue
//
//    FallThrough (limite de 64 atingido):
//      mov [cpu.pc], <end_pc>
//      mov rax, <ExitInfo::FallThrough>

// 4) epílogo:
restore callee-saved; pop rbp; ret
```

`ExitInfo` discriminante (em `rax` no retorno): `Continue` (retornar ao dispatcher), `AwaitingInput`, `Halt`, `Fault`.

### Trampolines: `extern "C"` adapters

`dynasm` não pode chamar métodos Rust de `&mut self` diretamente. Cada callout vira um `extern "C" fn(*mut CacheController, ...) -> ret`:

```rust
unsafe extern "C" fn jit_dcache_read32(mem: *mut CacheController, addr: u32) -> u32 {
    let mem = unsafe { &mut *mem };
    mem.dcache_read32(addr).unwrap_or(0)
    // TODO: handle FalconError — propagar como ExitInfo::Fault
}
```

Lista mínima de trampolines pra Fase B (apenas ALU + loads/stores integer):
`jit_fetch32`, `jit_dcache_read{8,16,32}`, `jit_store{8,16,32}`, `jit_add_instruction_cycles`, `jit_snapshot_stats`, `jit_handle_syscall`.

### Validação

Novo backend `JitInterpreterBackend` (modo de validação interno, não exposto via CLI):

```rust
// src/falcon/jit/validate.rs (#[cfg(test)])
//
// Toda step: compila o bloco a partir de cpu.pc, executa, compara cpu/mem/console
// com o interpretador puro. Qualquer divergência → panic com o PC e a instrução.
```

Teste de integração roda `rust-to-raven.elf` e `c-to-raven.elf` ponta a ponta no modo de validação, comparando bit-a-bit com o interpretador.

### Não compilados na Fase B (callout sempre)

- RV32F (todas as instruções de float)
- RV32A (AMO atômicos)
- `Fence`, `FenceI`, `Ecall`, `Ebreak`

São terminadores de bloco; o bloco encerra antes deles e o interpretador resolve.

### Critério de sucesso

- `cargo test --features jit` 100% verde.
- `JitInterpreterBackend` roda `rust-to-raven.elf` e `c-to-raven.elf` sem divergência.
- Microbench (loop de soma de array) mostra speedup ≥ 2× vs interpretador.

---

## Fase C — Modos `hot` e `full` em produção

### `--jit=hot` (política seletiva)

`HotBackend` mantém duas estruturas:

```rust
pub struct HotBackend {
    interpreter: InterpreterBackend,
    cache: CompiledBlockCache,
    profile: HotProfile,        // referência compartilhada com o interpretador
    threshold: u32,             // = 500
}
```

`run_until_yield` por step:
1. `cache.get(cpu.pc)` → se hit, executa o bloco compilado, atualiza `cpu` e métricas via trampolines.
2. Miss + `profile.get(cpu.pc) >= 500` → compila o bloco (síncrono), insere no cache, retry.
3. Caso contrário → delega ao `interpreter.run_until_yield`. O interpretador continua atualizando `HotProfile` no hook existente.

Custo de compilação: amortizado. Para um loop quente de 100k iterações, são ~500 iterações interpretadas + ~99500 compiladas.

### `--jit=full` (varredura eager)

`FullBackend` faz scan format-agnostic ao construir:

```rust
pub fn new(cpu: &Cpu, mem: &CacheController) -> Self {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(cpu.pc);  // ← format-agnostic: nunca lê ElfInfo
    
    while let Some(pc) = queue.pop_front() {
        if !visited.insert(pc) { continue; }
        let block = scan_block(mem, pc);
        match block.terminator {
            Branch | Jal => {
                queue.push_back(target_estatico_via_imm);
                queue.push_back(block.end_pc + 4);  // fall-through
            }
            Jalr => { /* target dinâmico — fica pra resolução em runtime */ }
            Ecall | Ebreak | Halt | Fence => { /* terminator absoluto, não continua */ }
            FallThrough => queue.push_back(block.end_pc + 4),
        }
        compiled.push(compile(&block));
    }
    
    Self { cache: compiled, ... }
}
```

Pontos importantes:
- O scan trabalha em cima do que o loader (ELF/FALC/ASM/flat) já depositou em `mem.ram`. Nenhum código por trás precisa diferenciar formatos.
- JALR / blocos não-descobertos em tempo de scan: cache miss em runtime → fallback ao interpretador. Versão futura pode compilar sob demanda (mesclando com hot).
- Tabela de despacho: `HashMap<u32, Arc<CompiledBlock>>` para resolução de JAL/JALR target em runtime. Lookup constante.

### SMC — `FALCON_MAP_EXEC`

O driver de execução já consome `cpu.pending_exec_map`. Após drenar:

```rust
if let Some(region) = cpu.pending_exec_map.take() {
    backend.invalidate(region.start, region.end);
}
```

`HotBackend` e `FullBackend` implementam `invalidate` → `cache.invalidate_range`. Próximo despacho de um PC nessa região cai pro interpretador (`hot`) ou recompila (`full`).

### Polish opcional

- **Per-hart `HotProfile`**: na Fase A o profile é global. Se workloads multi-hart com perfis distintos virarem dor, mover para `HartCoreRuntime`. Decidir com base em medição.
- **Enum dispatch vs `Box<dyn>`**: substituir `Box<dyn ExecutionBackend>` por `enum Backend { Interp(...), Hot(...), Full(...) }` se microbench mostrar que a indireção custa > 5% no modo interpretador puro.
- **Toggle na TUI**: adicionar um menu em Settings pra alternar backend em runtime (requer flush do cache atual ao trocar).

### Critério de sucesso

- `--jit=hot rust-to-raven.elf`: speedup ≥ 2× vs `--jit=none` em programas com loops dominantes (bubble sort, fatorial, hash).
- `--jit=full rust-to-raven.elf`: speedup similar ao hot após warm-up, sem warm-up no startup (tradução one-shot).
- Todas as métricas (cache hits/misses, CPI, pipeline cycles quando aplicável) idênticas a `--jit=none` em runs determinísticos.
- Suite de testes 100% verde nos 3 modos.

---

## Limites conhecidos

- **Pipeline mode** (`--pipeline`) continua fora do trait `ExecutionBackend`. Pipeline é um simulador de stages, não um candidato a codegen. Tentar pluggar o JIT no pipeline inverteria a arquitetura.
- **RV32F + RV32A**: callout sempre. Codegen nativo de float (SSE/NEON) é Fase D ou nunca, dependendo da demanda.
- **Self-modifying code denso**: invalidação por intervalo é O(N) sobre os blocos cacheados. Programas que remapeiam executável a cada poucas instruções verão degradação. Não é o caso de uso real.

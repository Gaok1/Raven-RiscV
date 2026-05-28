# Memória virtual (Sv32)

> [Read in English](../en/virtual-memory.md)

O Raven implementa o esquema **Sv32** do RISC-V: uma tabela de páginas de 2 níveis percorrida por software pela MMU do processador, fronteada por uma TLB configurável. A tradução vem **desligada por padrão** para que programas antigos rodem sem mudança; ligue em **Settings → Virtual Memory** (também persistido no `.rcfg`).

Quando ligada, todo fetch e todo load/store passa pela MMU. M-mode continua sempre em endereçamento físico; U-mode é traduzido assim que `satp.MODE = Sv32`. A TLB tem sua própria subtab dentro da aba **Cache** onde você configura número de entradas, associatividade, política de substituição, latência de hit e penalidade de miss — e vê hits/misses ao vivo.

---

## Quando ligar

| Caso de uso | VM ligada? |
|-------------|------------|
| Programa RV32IMAF simples, memória plana | desligada — mesmo comportamento de antes |
| Estudar walk de tabela de páginas, bits A/D, page faults | **ligada** |
| Comparar penalidades de TLB entre políticas de substituição | **ligada** |
| Rodar um kernel estilo OS (setup em M-mode + código user em U-mode) | **ligada** |

O toggle fica em `Settings → Virtual Memory`. Default é **desligada**. Com VM desligada a MMU é totalmente bypassed — sem lookup de TLB, sem walker, sem ciclos extras.

---

## Tradução de endereços

O Sv32 divide o endereço virtual de 32 bits em dois índices de 10 bits para a tabela de páginas e um offset de 12 bits:

```
 31         22 21         12 11          0
┌─────────────┬─────────────┬─────────────┐
│   VPN[1]    │   VPN[0]    │   offset    │
└─────────────┴─────────────┴─────────────┘
```

A tradução percorre dois níveis de PTEs começando em `satp.PPN << 12`:

1. **PTE L1** em `(satp.PPN << 12) + VPN[1] * 4` — se for leaf (R/W/X setado), a página é uma **megapágina** de 4 MiB e `VPN[0]` vira parte do offset.
2. **PTE L0** em `(L1.PPN << 12) + VPN[0] * 4` — precisa ser leaf. O endereço físico final da página de 4 KiB é `(L0.PPN << 12) | offset`.

Os bits `A` (accessed) e `D` (dirty) são setados automaticamente pelo walker quando o acesso bem-sucedido — o Raven **não** vetoriza pra OS atualizar, então dá pra experimentar sem escrever um fault handler.

---

## Níveis de privilégio

| Modo | Notação | Comportamento com `satp.MODE = Sv32` |
|------|---------|--------------------------------------|
| Machine | M | Sempre físico, MMU bypassada |
| User    | U | Sempre traduzido; PTEs com `U=0` faltam |

Um trap (page fault, ecall, ebreak) coloca a CPU em M-mode e salva o modo anterior em `mstatus.MPP`. O `mret` restaura o modo salvo e retoma em `mepc`.

---

## Traps de page fault

Quando a tradução falha, a CPU levanta uma das causas:

| Cause | Significado |
|-------|-------------|
| `12` | Instruction page fault — fetch não pôde ser traduzido |
| `13` | Load page fault — `lw`/`lh`/`lb` não pôde ser traduzido |
| `15` | Store page fault — `sw`/`sh`/`sb` não pôde ser traduzido |

O trap preenche `mcause`, `mtval` (endereço virtual que faltou), `mepc` (PC que faltou), seta `mstatus.MPP` para o modo anterior, troca pra M-mode e pula pra `mtvec & ~3`. Com `mtvec = 0`, o Raven imprime a falha no console e para — útil quando você esquece de instalar um handler.

---

## CSRs e instruções de sistema

O Raven implementa o mínimo de Zicsr + ops privilegiadas pra rodar Sv32:

| CSR    | Número | Uso |
|--------|--------|-----|
| `satp` | `0x180` | PPN da raiz da tabela + ASID + MODE (1 = Sv32, 0 = Bare) |
| `mstatus` | `0x300` | Bits de privilégio salvo (`MPP`) na entrada/saída do trap |
| `mtvec` | `0x305` | Endereço base do vetor de trap |
| `mepc`  | `0x341` | PC salvo no trap |
| `mcause`| `0x342` | Causa do trap |
| `mtval` | `0x343` | Valor específico do trap (vaddr da falha em page faults) |

Instruções: `csrrw / csrrs / csrrc` (e as variantes `i` com imediato), `mret`, `sfence.vma`. Escrita em `satp` flusha a TLB; `sfence.vma` também flusha (ignorando os filtros `rs1`/`rs2` neste release).

---

## Configurando a TLB

A UI da TLB fica em **Cache → TLB** com três subviews:

- **Stats** — taxa de hit, totais de hits/misses, page faults e histórico rolante de 300 ciclos.
- **Config** — entries (potência de 2), associatividade, política de substituição (LRU / FIFO / Random / Clock / LFU / MRU), latência de hit, penalidade de miss. Aplicar pra commitar.
- **Entries** — tabela por entrada: VPN | PPN | RWXU | ASID | V | G | A | D | megapágina.

A configuração persiste no `.rcfg` (via export/import do Cache) pra você levar um layout de TLB junto com seus configs de CPI e cache.

---

## Impacto em performance

Todo fetch e load/store recebe duas parcelas de latência da MMU:

- **Hit:** `tlb.hit_latency` ciclos (default `1`).
- **Miss:** `tlb.miss_penalty` ciclos pro walk (default `20`), mais quaisquer ciclos extras que o walker gaste lendo PTEs da RAM.

No **pipeline mode** o stall da MMU cai em `if_stall_cycles` ou `mem_stall_cycles` no slot correspondente — aparece como faixas vermelhas de MEM/IF no Gantt. No **interpreter mode** o stall vai pra `extra_cycles` e reflete em `total_program_cycles` / CPI.

---

## Sequência mínima de boot pra U-mode

```asm
# Constrói um mapeamento único de 4 KiB VA 0x10_0000 → PA 0x8_0000 (R|W|U)
# e cai em U-mode na página recém-mapeada.

.text
boot:
    # ... montar PTEs raiz + leaf na RAM (ver tests/mmu_traps.rs pro layout)

    li   t0, 0x80000000     # satp.MODE = Sv32 (bit 31) | PPN da raiz
    la   t1, root_pt
    srli t1, t1, 12
    or   t0, t0, t1
    csrw satp, t0           # escritas aqui flusham a TLB

    la   t0, user_entry
    csrw mepc, t0
    li   t0, 0              # mstatus.MPP = U
    csrw mstatus, t0
    mret                    # cai em U-mode em user_entry
```

O fluxo completo — layout de PTE, roteamento de fault via `mtvec`, `mret` de volta pra U-mode — está exercitado em `tests/mmu_traps.rs`.

---

## Veja também

- [Mapa de memória](memory-allocation.md) — layout de endereços físicos usado como backing store
- [Config de cache](cache-config.md) — campos do `.fcache` / `.rcfg` incluindo o bloco `[tlb]`
- [Simulação de pipeline](pipeline.md) — onde os stalls da MMU aparecem no Gantt

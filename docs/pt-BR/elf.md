# Como o Falcon carrega um arquivo ELF

> Documento didático — assume que você não sabe nada sobre ELF. :)

---

## 1. O que é um arquivo ELF?

Quando você compila um programa em C ou Rust para Linux (ou para um target bare-metal como `riscv32im-unknown-none-elf`), o resultado não é só "os bytes das instruções". O compilador gera um arquivo **ELF** (*Executable and Linkable Format*) — um container estruturado que diz ao sistema operacional (ou ao simulador) *onde* colocar cada pedaço do programa na memória, *qual endereço* é o ponto de entrada, e muitas outras informações.

Pense num arquivo ELF como um baú com gavetas:

```
┌─────────────────────────────────┐
│         ELF Header              │  ← "mapa" do baú: onde fica cada gaveta
├─────────────────────────────────┤
│     Program Headers (Segments)  │  ← o que carregar na memória e onde
├─────────────────────────────────┤
│        .text   (código)         │  ← bytes das instruções
│        .rodata (constantes)     │
│        .data   (variáveis init) │
│        .bss    (variáveis zero) │  ← existe no header, mas não ocupa bytes no arquivo
├─────────────────────────────────┤
│     Section Headers             │  ← metadados para o linker/debugger
│       .symtab  (símbolos)       │
│       .strtab  (nomes)          │
└─────────────────────────────────┘
```

O **Falcon** (o núcleo de simulação do Raven) só precisa de duas coisas para executar um programa:

1. Copiar os bytes certos para os endereços certos na RAM simulada.
2. Saber em qual endereço começar a executar (o *entry point*).

As seções (`Section Headers`) são informação extra — úteis para debugger, mas não necessárias para executar.

---

## 2. O ELF Header — os primeiros 52 bytes

Todo arquivo ELF começa com um header de tamanho fixo. No ELF32 (32 bits) ele tem **52 bytes**. O Falcon lê assim:

```
Offset  Tamanho  Campo           Valor esperado / uso
──────  ───────  ──────────────  ────────────────────────────────────────────
0       4        e_ident[magic]  7f 45 4c 46  ← "\x7fELF" em ASCII
4       1        EI_CLASS        1            ← ELF32 (2 seria ELF64)
5       1        EI_DATA         1            ← little-endian
18      2        e_machine       0xF3         ← RISC-V (243 decimal)
24      4        e_entry         ex: 0x110d4  ← endereço do _start
28      4        e_phoff         offset dos Program Headers no arquivo
42      2        e_phentsize     tamanho de cada Program Header (≥ 32)
44      2        e_phnum         quantos Program Headers existem
32      4        e_shoff         offset dos Section Headers no arquivo
46      2        e_shentsize     tamanho de cada Section Header (≥ 40)
48      2        e_shnum         quantos Section Headers existem
50      2        e_shstrndx      índice da seção com os nomes das seções
```

No código (`src/falcon/program/elf.rs`):

```rust
let e_entry     = u32le(24);   // onde começar a executar
let e_phoff     = u32le(28);   // onde no arquivo ficam os Program Headers
let e_phentsize = u16le(42);   // tamanho de cada entrada
let e_phnum     = u16le(44);   // quantas entradas

let e_shoff     = u32le(32);   // onde ficam os Section Headers
let e_shentsize = u16le(46);
let e_shnum     = u16le(48);
let e_shstrndx  = u16le(50);   // qual seção contém os nomes
```

Se o magic for errado, a classe não for 1 (ELF32), ou a máquina não for `0xF3` (RISC-V), o Falcon rejeita o arquivo com erro.

---

## 3. Program Headers — o que vai para a memória

Os **Program Headers** (também chamados de *segmentos*) descrevem o que o loader precisa fazer. Cada entrada tem **32+ bytes**:

```
Offset  Tamanho  Campo      Significado
──────  ───────  ─────────  ────────────────────────────────────────────────
0       4        p_type     tipo do segmento (1 = PT_LOAD = "carregue isso")
4       4        p_offset   onde no *arquivo* começa esse segmento
8       4        p_vaddr    endereço virtual na *memória* onde deve ir
16      4        p_filesz   quantos bytes copiar do arquivo para a memória
20      4        p_memsz    tamanho total na memória (pode ser > filesz → BSS)
24      4        p_flags    permissões: bit 0 = executável (X), bit 1 = W, bit 2 = R
```

O Falcon só se importa com segmentos do tipo **PT_LOAD** (tipo 1). Para cada um:

```rust
if p_type != PT_LOAD { continue; }  // ignora PT_DYNAMIC, PT_NOTE, etc.

// 1. Copia p_filesz bytes do arquivo para p_vaddr na RAM
load_bytes(mem, p_vaddr, &bytes[p_offset .. p_offset + p_filesz]);

// 2. Se p_memsz > p_filesz, zera o resto (isso é o BSS!)
if p_memsz > p_filesz {
    zero_bytes(mem, p_vaddr + p_filesz, p_memsz - p_filesz);
}
```

### O BSS na prática

O BSS são variáveis globais inicializadas com zero. Como "zero" não precisa ser armazenado no arquivo, o linker coloca `p_filesz < p_memsz` — a diferença é a quantidade de bytes a zerar. O Raven faz isso automaticamente, por isso o `_start` do `hello-raven` não precisa mais zerar o BSS manualmente.

### Identificando .text vs .data

```rust
if p_flags & PF_X != 0 {
    // tem bit executável → é o segmento de código (.text)
    text_bytes = ...;
    text_base  = p_vaddr;
} else {
    // não executável → é o segmento de dados (.data/.bss)
    data_base = p_vaddr;
}
```

Um ELF RISC-V típico tem dois PT_LOAD:

```
PT_LOAD #1  flags=R+X  → .text + .rodata  (código e constantes)
PT_LOAD #2  flags=R+W  → .data + .bss     (variáveis)
```

### Heap start

Depois de carregar todos os segmentos, o Falcon calcula onde a heap começa:

```rust
let end = p_vaddr + p_memsz;   // fim deste segmento na memória
if end > seg_end_max { seg_end_max = end; }

// depois do loop:
let heap_start = (seg_end_max + 15) & !15;  // alinha pra cima em 16 bytes
```

Esse valor é guardado em `cpu.heap_break` e é o que o syscall `brk` retorna na primeira chamada.

---

## 4. Section Headers — metadados

Os **Section Headers** não são necessários para executar, mas contêm informações valiosas. O Falcon os lê para extrair a tabela de símbolos.

Cada Section Header tem **40 bytes**:

```
Offset  Tamanho  Campo       Significado
──────  ───────  ──────────  ──────────────────────────────────────────
0       4        sh_name     índice em shstrtab (onde fica o nome da seção)
4       4        sh_type     tipo: 2=SHT_SYMTAB, 3=SHT_STRTAB, 8=SHT_NOBITS
12      4        sh_addr     endereço virtual (0 se não vai para a memória)
16      4        sh_offset   onde no arquivo ficam os bytes da seção
20      4        sh_size     quantos bytes
24      4        sh_link     para .symtab: índice da .strtab associada
36      4        sh_entsize  tamanho de cada entrada (16 para .symtab)
```

### shstrtab — a agenda de nomes

Para saber o nome de uma seção, o campo `sh_name` é um **offset** numa seção especial chamada **shstrtab** (*section header string table*). O índice dessa seção está em `e_shstrndx` no ELF header.

```
shstrtab (bytes):  \0.text\0.data\0.bss\0.symtab\0...
                    ^  ^           ^
                    0  1           12  ← sh_name = 12 → nome = ".bss"
```

---

## 5. A tabela de símbolos — como os nomes chegam no disassembly

A seção `.symtab` (tipo `SHT_SYMTAB`) contém uma lista de símbolos — funções, variáveis globais, labels. Cada entrada tem **16 bytes**:

```
Offset  Tamanho  Campo     Significado
──────  ───────  ────────  ────────────────────────────────────────────────
0       4        st_name   offset do nome na .strtab associada
4       4        st_value  endereço virtual do símbolo
8       4        st_size   tamanho em bytes (0 se desconhecido)
12      1        st_info   tipo nos 4 bits baixos: 1=OBJECT, 2=FUNC
13      1        st_other  visibilidade (ignoramos)
14      2        st_shndx  em qual seção o símbolo vive
```

O Falcon filtra os símbolos que importam:

```rust
let sym_type = st_info & 0x0F;
if sym_type != STT_FUNC && sym_type != STT_OBJECT { continue; }  // só funções e variáveis
if st_value == 0 { continue; }             // símbolo sem endereço (externo/undef)
if name.is_empty() || name.starts_with('$') { continue; }  // nomes internos do linker
```

O resultado vai para `run.labels: HashMap<u32, Vec<String>>` — o mesmo mapa que o assembler popula quando você escreve um label em ASM. Por isso o disassembly mostra `<main>:`, `<factorial>:` etc. ao carregar um ELF compilado.

---

## 6. Layout de memória no Raven

Depois de carregar um ELF típico do `hello-raven`, a memória de 128 KB fica assim:

```
Endereço        Conteúdo
──────────────  ──────────────────────────────────────────────────
0x00000000      (vazio — trap se executar)
...
0x00010000      .rodata + .data  (PT_LOAD #1, flags=R)
                .bss             (zeroed por p_memsz > p_filesz)
── heap_break ──────────────────────────────────────────────────── ← cpu.heap_break
                heap (cresce para cima via brk)
...             (espaço livre)
...
0x0001FFFC      stack (cresce para baixo; sp = 0x20000 na init)
0x00020000      (fim da RAM — 128 KB)
```

O segmento de código `.text` vai para um endereço separado porque o linker default do RISC-V coloca rodata/data em `0x10000` e `.text` em `0x110xx` (logo após, noutra página).

---

## 7. Fluxo completo resumido

```
arquivo ELF
    │
    ▼
load_elf(bytes, mem)
    │
    ├─ valida magic, class, data, machine
    │
    ├─ lê ELF Header → entry, phoff, shoff, ...
    │
    ├─ loop PT_LOAD segments
    │       ├─ copia p_filesz bytes → mem[p_vaddr]
    │       ├─ zera (p_memsz - p_filesz) bytes → BSS
    │       ├─ identifica .text (PF_X) e .data
    │       └─ atualiza seg_end_max
    │
    ├─ heap_start = align16(seg_end_max)
    │
    ├─ parse_sections() ─────────────────────────────────────┐
    │       ├─ lê todos Section Headers                      │
    │       ├─ acha shstrtab (nomes das seções)              │
    │       ├─ acha .symtab → itera símbolos                 │
    │       │       └─ filtra FUNC/OBJECT → symbols HashMap  │
    │       └─ coleta .data/.rodata/.bss → sections Vec      │
    │                                                         │
    └─ retorna ElfInfo ◄──────────────────────────────────────┘
            ├─ entry, text_base, text_bytes, data_base
            ├─ total_bytes, heap_start
            ├─ symbols  → run.labels (aparecem no disassembly)
            └─ sections → run.elf_sections (painel lateral)
```

---

## 8. Como inspecionar um ELF você mesmo

Você tem o binário do `hello-raven` em `hello-raven/target/riscv32im-unknown-none-elf/release/hello-raven`. Para ver o que o Falcon vai ler:

```bash
# ELF header completo
readelf -h hello-raven

# Program headers (segmentos que o loader usa)
readelf -l hello-raven

# Section headers (metadados)
readelf -S hello-raven

# Tabela de símbolos (o que vira label no disassembly)
readelf -s hello-raven

# Disassembly com labels
objdump -d hello-raven
```

Exemplo de saída do `readelf -l`:

```
Program Headers:
  Type    Offset   VirtAddr   PhysAddr   FileSiz  MemSiz   Flg  Align
  LOAD    0x001000 0x00010000 0x00010000 0x00050  0x00060  RW   0x1000
  LOAD    0x002000 0x000110d4 0x000110d4 0x00400  0x00400  R E  0x1000
            ↑            ↑                  ↑        ↑      ↑
         no arquivo   na memória         bytes    bytes  R=leitura
                                        copiados  total  W=escrita
                                                         E=execução
```

O segundo segmento tem `FileSiz == MemSiz` (sem BSS), flags `R E` (leitura + execução) → esse é o `.text` que o Falcon identifica como código.

---

## Referência rápida de números mágicos

| Constante     | Valor  | Significado                        |
|---------------|--------|------------------------------------|
| `PT_LOAD`     | 1      | segmento a carregar na memória     |
| `PF_X`        | 1      | flag executável no p_flags         |
| `SHT_SYMTAB`  | 2      | seção é uma tabela de símbolos     |
| `SHT_STRTAB`  | 3      | seção é uma string table           |
| `SHT_NOBITS`  | 8      | seção sem bytes no arquivo (BSS)   |
| `STT_OBJECT`  | 1      | símbolo é uma variável/dado        |
| `STT_FUNC`    | 2      | símbolo é uma função               |
| `EM_RISCV`    | 0xF3   | e_machine para RISC-V              |
| `ELFCLASS32`  | 1      | EI_CLASS para 32 bits              |
| `ELFDATA2LSB` | 1      | EI_DATA para little-endian         |

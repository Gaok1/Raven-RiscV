# Como o Raven carrega um arquivo ELF

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

Para executar um programa, o Raven só precisa de duas coisas:

1. Copiar os bytes certos para os endereços certos na RAM simulada.
2. Saber em qual endereço começar a executar (o *entry point*).

As seções (`Section Headers`) são informação extra — úteis para debugger, mas não necessárias para executar.

---

## 2. O ELF Header — os primeiros 52 bytes

Todo arquivo ELF começa com um header de tamanho fixo. No ELF32 (32 bits) ele tem **52 bytes**. O Raven lê estes campos dele:

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

O header é o que diz ao loader onde tudo o mais está: `e_entry` é onde a execução começa, `e_phoff` aponta para a tabela de segmentos a carregar, e `e_shoff` aponta para os metadados das seções. Todo valor de múltiplos bytes aqui é little-endian.

Se o magic for errado, a classe não for 1 (ELF32), ou a máquina não for `0xF3` (RISC-V), o Raven rejeita o arquivo com erro.

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

Só os segmentos do tipo **PT_LOAD** (tipo 1) importam para a execução; outros tipos (`PT_DYNAMIC`, `PT_NOTE`, …) são ignorados. Para cada segmento PT_LOAD, o loader faz duas coisas:

1. **Copia** `p_filesz` bytes do arquivo (a partir de `p_offset`) para o endereço `p_vaddr` na RAM.
2. **Zera o resto:** se `p_memsz > p_filesz`, preenche os `p_memsz − p_filesz` bytes extras com zero. Essa diferença é o BSS.

### O BSS na prática

O BSS são variáveis globais inicializadas com zero. Como "zero" não precisa ser armazenado no arquivo, o linker coloca `p_filesz < p_memsz` — a diferença é a quantidade de bytes a zerar. O Raven faz isso automaticamente, por isso o `_start` do `hello-raven` não precisa mais zerar o BSS manualmente.

### Identificando .text vs .data

Os bits de permissão em `p_flags` dizem o que um segmento é:

- Se o bit **executável** (`PF_X`) está ligado, o segmento contém código → é o `.text` (mais o `.rodata`).
- Se **não** é executável, o segmento contém dados → é o `.data` / `.bss`.

Um ELF RISC-V típico tem dois PT_LOAD:

```
PT_LOAD #1  flags=R+X  → .text + .rodata  (código e constantes)
PT_LOAD #2  flags=R+W  → .data + .bss     (variáveis)
```

### Heap start

Depois de carregar todos os segmentos, o simulador olha o maior endereço de memória que algum segmento alcançou (`p_vaddr + p_memsz` do último), arredonda para cima até um limite de 16 bytes, e usa isso como o **início da heap**. É o endereço que o syscall `brk` retorna na primeira chamada, e onde a alocação dinâmica começa a crescer para cima.

---

## 4. Section Headers — metadados

Os **Section Headers** não são necessários para executar, mas contêm informações valiosas. O Raven os lê para extrair a tabela de símbolos.

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

Nem todo símbolo é útil para o disassembly, então o loader guarda só os que nomeiam código ou dados reais. Um símbolo é mantido quando:

- seu tipo é uma **função** (`STT_FUNC`) ou uma **variável/objeto** (`STT_OBJECT`), e
- ele tem um endereço real (`st_value ≠ 0`), e
- seu nome não é vazio e não é um nome interno do linker (esses começam com `$`).

Os símbolos mantidos viram uma tabela que mapeia cada endereço para um nome — o mesmo tipo de tabela que o assembler constrói quando você escreve um label em ASM. Por isso o disassembly mostra `<main>:`, `<factorial>:` etc. ao carregar um ELF compilado.

---

## 6. Layout de memória no Raven

Depois de carregar um ELF típico do `hello-raven`, a memória (16 MiB por padrão, configurável) fica assim:

```
Endereço        Conteúdo
──────────────  ──────────────────────────────────────────────────
0x00000000      (vazio — trap se executar)
...
0x00010000      .rodata + .data  (PT_LOAD #1, flags=R)
                .bss             (zerado por p_memsz > p_filesz)
── início heap ────────────────────────────────────────────────────
                heap (cresce para cima via brk)
...             (espaço livre)
...
RAM_SIZE-4      stack (grows downward; sp = RAM_SIZE at init)
RAM_SIZE        (fim da RAM configurada)
```

O segmento de código `.text` vai para um endereço separado porque o linker default do RISC-V coloca rodata/data em `0x10000` e `.text` em `0x110xx` (logo após, noutra página).

---

## 7. Fluxo completo resumido

```
arquivo ELF
    │
    ▼
Carregar
    │
    ├─ valida magic, class, data, machine
    │
    ├─ lê ELF Header → entry, phoff, shoff, ...
    │
    ├─ loop PT_LOAD segments
    │       ├─ copia p_filesz bytes → mem[p_vaddr]
    │       ├─ zera (p_memsz - p_filesz) bytes → BSS
    │       ├─ identifica .text (PF_X) e .data
    │       └─ guarda o maior endereço final
    │
    ├─ início heap = align16(maior endereço final)
    │
    ├─ ler seções ───────────────────────────────────────────┐
    │       ├─ lê todos Section Headers                      │
    │       ├─ acha shstrtab (nomes das seções)              │
    │       ├─ acha .symtab → itera símbolos                 │
    │       │       └─ filtra FUNC/OBJECT → tabela símbolos  │
    │       └─ coleta .data/.rodata/.bss → lista de seções   │
    │                                                         │
    └─ pronto para executar ◄─────────────────────────────────┘
            ├─ entry, base do código, bytes do código, base dos dados
            ├─ tamanho total, início da heap
            ├─ símbolos → labels (aparecem no disassembly)
            └─ seções  → lista de seções (painel lateral)
```

---

## 8. Como inspecionar um ELF você mesmo

Você tem o binário do `hello-raven` em `hello-raven/target/riscv32im-unknown-none-elf/release/hello-raven`. Para ver o que o loader vai ler:

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

O segundo segmento tem `FileSiz == MemSiz` (sem BSS), flags `R E` (leitura + execução) → esse é o `.text` identificado como código.

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

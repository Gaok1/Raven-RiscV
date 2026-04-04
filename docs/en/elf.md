# How Falcon loads an ELF file

> Educational document — assumes you know nothing about ELF.

---

## 1. What is an ELF file?

When you compile a C or Rust program for Linux (or for a bare-metal target like `riscv32im-unknown-none-elf`), the result is not just "the instruction bytes". The compiler produces an **ELF** (*Executable and Linkable Format*) file — a structured container that tells the operating system (or the simulator) *where* to place each piece of the program in memory, *which address* is the entry point, and much more.

Think of an ELF file as a chest with drawers:

```
┌─────────────────────────────────┐
│         ELF Header              │  ← "map" of the chest: where each drawer is
├─────────────────────────────────┤
│     Program Headers (Segments)  │  ← what to load into memory and where
├─────────────────────────────────┤
│        .text   (code)           │  ← instruction bytes
│        .rodata (constants)      │
│        .data   (init variables) │
│        .bss    (zero variables) │  ← exists in the header but takes no bytes in the file
├─────────────────────────────────┤
│     Section Headers             │  ← metadata for the linker/debugger
│       .symtab  (symbols)        │
│       .strtab  (names)          │
└─────────────────────────────────┘
```

**Falcon** (the Raven simulation core) only needs two things to execute a program:

1. Copy the right bytes to the right addresses in simulated RAM.
2. Know which address to start executing at (the *entry point*).

Sections (`Section Headers`) are extra information — useful for a debugger, but not required to run.

---

## 2. The ELF Header — the first 52 bytes

Every ELF file starts with a fixed-size header. In ELF32 (32-bit) it is **52 bytes**. Falcon reads it like this:

```
Offset  Size  Field            Expected value / use
──────  ────  ───────────────  ────────────────────────────────────────────
0       4     e_ident[magic]   7f 45 4c 46  ← "\x7fELF" in ASCII
4       1     EI_CLASS         1            ← ELF32 (2 would be ELF64)
5       1     EI_DATA          1            ← little-endian
18      2     e_machine        0xF3         ← RISC-V (243 decimal)
24      4     e_entry          e.g. 0x110d4 ← address of _start
28      4     e_phoff          offset of Program Headers in the file
42      2     e_phentsize      size of each Program Header (≥ 32)
44      2     e_phnum          how many Program Headers exist
32      4     e_shoff          offset of Section Headers in the file
46      2     e_shentsize      size of each Section Header (≥ 40)
48      2     e_shnum          how many Section Headers exist
50      2     e_shstrndx       index of the section containing section names
```

In the code (`src/falcon/program/elf.rs`):

```rust
let e_entry     = u32le(24);   // where to start executing
let e_phoff     = u32le(28);   // where in the file the Program Headers are
let e_phentsize = u16le(42);   // size of each entry
let e_phnum     = u16le(44);   // how many entries

let e_shoff     = u32le(32);   // where Section Headers are
let e_shentsize = u16le(46);
let e_shnum     = u16le(48);
let e_shstrndx  = u16le(50);   // which section contains names
```

If the magic is wrong, the class is not 1 (ELF32), or the machine is not `0xF3` (RISC-V), Falcon rejects the file with an error.

---

## 3. Program Headers — what goes into memory

**Program Headers** (also called *segments*) describe what the loader needs to do. Each entry has **32+ bytes**:

```
Offset  Size  Field      Meaning
──────  ────  ─────────  ────────────────────────────────────────────────
0       4     p_type     segment type (1 = PT_LOAD = "load this")
4       4     p_offset   where in the *file* this segment starts
8       4     p_vaddr    virtual address in *memory* where it should go
16      4     p_filesz   how many bytes to copy from the file to memory
20      4     p_memsz    total size in memory (can be > filesz → BSS)
24      4     p_flags    permissions: bit 0 = executable (X), bit 1 = W, bit 2 = R
```

Falcon only cares about segments of type **PT_LOAD** (type 1). For each one:

```rust
if p_type != PT_LOAD { continue; }  // skip PT_DYNAMIC, PT_NOTE, etc.

// 1. Copy p_filesz bytes from the file to p_vaddr in RAM
load_bytes(mem, p_vaddr, &bytes[p_offset .. p_offset + p_filesz]);

// 2. If p_memsz > p_filesz, zero the rest (this is BSS!)
if p_memsz > p_filesz {
    zero_bytes(mem, p_vaddr + p_filesz, p_memsz - p_filesz);
}
```

### BSS in practice

BSS contains global variables initialized to zero. Since "zero" does not need to be stored in the file, the linker sets `p_filesz < p_memsz` — the difference is the number of bytes to zero. Raven does this automatically, so the `_start` of programs no longer needs to zero BSS manually.

### Identifying .text vs .data

```rust
if p_flags & PF_X != 0 {
    // has executable bit → this is the code segment (.text)
    text_bytes = ...;
    text_base  = p_vaddr;
} else {
    // not executable → this is the data segment (.data/.bss)
    data_base = p_vaddr;
}
```

A typical RISC-V ELF has two PT_LOAD segments:

```
PT_LOAD #1  flags=R+X  → .text + .rodata  (code and constants)
PT_LOAD #2  flags=R+W  → .data + .bss     (variables)
```

### Heap start

After loading all segments, Falcon calculates where the heap begins:

```rust
let end = p_vaddr + p_memsz;   // end of this segment in memory
if end > seg_end_max { seg_end_max = end; }

// after the loop:
let heap_start = (seg_end_max + 15) & !15;  // align up to 16 bytes
```

This value is stored in `cpu.heap_break` and is what the `brk` syscall returns on the first call.

---

## 4. Section Headers — metadata

**Section Headers** are not required to execute, but contain valuable information. Falcon reads them to extract the symbol table.

Each Section Header has **40 bytes**:

```
Offset  Size  Field       Meaning
──────  ────  ──────────  ──────────────────────────────────────────
0       4     sh_name     index in shstrtab (where the section name is)
4       4     sh_type     type: 2=SHT_SYMTAB, 3=SHT_STRTAB, 8=SHT_NOBITS
12      4     sh_addr     virtual address (0 if not loaded into memory)
16      4     sh_offset   where in the file the section bytes are
20      4     sh_size     how many bytes
24      4     sh_link     for .symtab: index of the associated .strtab
36      4     sh_entsize  size of each entry (16 for .symtab)
```

### shstrtab — the name directory

To find the name of a section, the `sh_name` field is an **offset** into a special section called **shstrtab** (*section header string table*). The index of that section is in `e_shstrndx` in the ELF header.

```
shstrtab (bytes):  \0.text\0.data\0.bss\0.symtab\0...
                    ^  ^           ^
                    0  1           12  ← sh_name = 12 → name = ".bss"
```

---

## 5. The symbol table — how names appear in the disassembly

The `.symtab` section (type `SHT_SYMTAB`) contains a list of symbols — functions, global variables, labels. Each entry has **16 bytes**:

```
Offset  Size  Field     Meaning
──────  ────  ────────  ────────────────────────────────────────────────
0       4     st_name   offset of the name in the associated .strtab
4       4     st_value  virtual address of the symbol
8       4     st_size   size in bytes (0 if unknown)
12      1     st_info   type in the low 4 bits: 1=OBJECT, 2=FUNC
13      1     st_other  visibility (ignored)
14      2     st_shndx  which section the symbol lives in
```

Falcon filters the symbols that matter:

```rust
let sym_type = st_info & 0x0F;
if sym_type != STT_FUNC && sym_type != STT_OBJECT { continue; }  // only functions and variables
if st_value == 0 { continue; }             // symbol without address (external/undef)
if name.is_empty() || name.starts_with('$') { continue; }  // linker-internal names
```

The result goes into `run.labels: HashMap<u32, Vec<String>>` — the same map the assembler populates when you write a label in ASM. That's why the disassembly shows `<main>:`, `<factorial>:` etc. when loading a compiled ELF.

---

## 6. Memory layout in Raven

After loading a typical ELF from `hello-raven`, the 128 KB memory looks like this:

```
Address         Contents
──────────────  ──────────────────────────────────────────────────
0x00000000      (empty — trap if executed)
...
0x00010000      .rodata + .data  (PT_LOAD #1, flags=R)
                .bss             (zeroed because p_memsz > p_filesz)
── heap_break ──────────────────────────────────────────────────── ← cpu.heap_break
                heap (grows upward via brk)
...             (free space)
...
0x0001FFFC      stack (grows downward; sp = 0x20000 at init)
0x00020000      (end of RAM — 128 KB)
```

The `.text` code segment goes to a separate address because the default RISC-V linker places rodata/data at `0x10000` and `.text` at `0x110xx` (right after, on another page).

---

## 7. Complete flow summary

```
ELF file
    │
    ▼
load_elf(bytes, mem)
    │
    ├─ validate magic, class, data, machine
    │
    ├─ read ELF Header → entry, phoff, shoff, ...
    │
    ├─ loop PT_LOAD segments
    │       ├─ copy p_filesz bytes → mem[p_vaddr]
    │       ├─ zero (p_memsz - p_filesz) bytes → BSS
    │       ├─ identify .text (PF_X) and .data
    │       └─ update seg_end_max
    │
    ├─ heap_start = align16(seg_end_max)
    │
    ├─ parse_sections() ─────────────────────────────────────┐
    │       ├─ read all Section Headers                      │
    │       ├─ find shstrtab (section names)                 │
    │       ├─ find .symtab → iterate symbols                │
    │       │       └─ filter FUNC/OBJECT → symbols HashMap  │
    │       └─ collect .data/.rodata/.bss → sections Vec     │
    │                                                         │
    └─ return ElfInfo ◄───────────────────────────────────────┘
            ├─ entry, text_base, text_bytes, data_base
            ├─ total_bytes, heap_start
            ├─ symbols  → run.labels (appear in disassembly)
            └─ sections → run.elf_sections (side panel)
```

---

## 8. Inspecting an ELF yourself

You have the `hello-raven` binary at `hello-raven/target/riscv32im-unknown-none-elf/release/hello-raven`. To see what Falcon will read:

```bash
# Full ELF header
readelf -h hello-raven

# Program headers (segments the loader uses)
readelf -l hello-raven

# Section headers (metadata)
readelf -S hello-raven

# Symbol table (what becomes a label in the disassembly)
readelf -s hello-raven

# Disassembly with labels
objdump -d hello-raven
```

Example output from `readelf -l`:

```
Program Headers:
  Type    Offset   VirtAddr   PhysAddr   FileSiz  MemSiz   Flg  Align
  LOAD    0x001000 0x00010000 0x00010000 0x00050  0x00060  RW   0x1000
  LOAD    0x002000 0x000110d4 0x000110d4 0x00400  0x00400  R E  0x1000
            ↑            ↑                  ↑        ↑      ↑
         in file    in memory            bytes    bytes  R=read
                                        copied   total  W=write
                                                        E=exec
```

The second segment has `FileSiz == MemSiz` (no BSS), flags `R E` (read + execute) → this is the `.text` that Falcon identifies as code.

---

## Quick reference — magic numbers

| Constant      | Value  | Meaning                            |
|---------------|--------|------------------------------------|
| `PT_LOAD`     | 1      | segment to load into memory        |
| `PF_X`        | 1      | executable flag in p_flags         |
| `SHT_SYMTAB`  | 2      | section is a symbol table          |
| `SHT_STRTAB`  | 3      | section is a string table          |
| `SHT_NOBITS`  | 8      | section with no bytes in file (BSS)|
| `STT_OBJECT`  | 1      | symbol is a variable/data          |
| `STT_FUNC`    | 2      | symbol is a function               |
| `EM_RISCV`    | 0xF3   | e_machine for RISC-V               |
| `ELFCLASS32`  | 1      | EI_CLASS for 32-bit                |
| `ELFDATA2LSB` | 1      | EI_DATA for little-endian          |

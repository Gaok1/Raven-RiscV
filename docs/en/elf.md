# How Raven loads an ELF file

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

To execute a program, Raven only needs two things:

1. Copy the right bytes to the right addresses in simulated RAM.
2. Know which address to start executing at (the *entry point*).

Sections (`Section Headers`) are extra information — useful for a debugger, but not required to run.

---

## 2. The ELF Header — the first 52 bytes

Every ELF file starts with a fixed-size header. In ELF32 (32-bit) it is **52 bytes**. Raven reads these fields from it:

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

The header is what tells the loader where everything else lives: `e_entry` is where execution starts, `e_phoff` points to the table of segments to load, and `e_shoff` points to the section metadata. Every multi-byte value here is little-endian.

If the magic is wrong, the class is not 1 (ELF32), or the machine is not `0xF3` (RISC-V), Raven rejects the file with an error.

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

Only segments of type **PT_LOAD** (type 1) matter for execution; other types (`PT_DYNAMIC`, `PT_NOTE`, …) are skipped. For each PT_LOAD segment, the loader does two things:

1. **Copy** `p_filesz` bytes from the file (starting at `p_offset`) to address `p_vaddr` in RAM.
2. **Zero the rest:** if `p_memsz > p_filesz`, fill the extra `p_memsz − p_filesz` bytes with zeros. That gap is the BSS.

### BSS in practice

BSS contains global variables initialized to zero. Since "zero" does not need to be stored in the file, the linker sets `p_filesz < p_memsz` — the difference is the number of bytes to zero. Raven does this automatically, so the `_start` of programs no longer needs to zero BSS manually.

### Identifying .text vs .data

The permission bits in `p_flags` say what a segment is:

- If the **executable** bit (`PF_X`) is set, the segment holds code → this is the `.text` (plus `.rodata`).
- If it is **not** executable, the segment holds data → this is `.data` / `.bss`.

A typical RISC-V ELF has two PT_LOAD segments:

```
PT_LOAD #1  flags=R+X  → .text + .rodata  (code and constants)
PT_LOAD #2  flags=R+W  → .data + .bss     (variables)
```

### Heap start

After loading all segments, the simulator looks at the highest memory address any segment reached (`p_vaddr + p_memsz` of the last one), rounds it up to a 16-byte boundary, and uses that as the **start of the heap**. This is the address the `brk` syscall returns on its first call, and where dynamic allocation begins growing upward.

---

## 4. Section Headers — metadata

**Section Headers** are not required to execute, but contain valuable information. Raven reads them to extract the symbol table.

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

Not every symbol is useful for the disassembly, so the loader keeps only the ones that name real code or data. A symbol is kept when:

- its type is a **function** (`STT_FUNC`) or a **variable/object** (`STT_OBJECT`), and
- it has a real address (`st_value ≠ 0`), and
- its name is non-empty and is not a linker-internal name (those start with `$`).

The kept symbols become a table mapping each address to a name — the same kind of table the assembler builds when you write a label in ASM. That's why the disassembly shows `<main>:`, `<factorial>:`, etc. when you load a compiled ELF.

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
── heap start ────────────────────────────────────────────────────
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
Load
    │
    ├─ validate magic, class, data, machine
    │
    ├─ read ELF Header → entry, phoff, shoff, ...
    │
    ├─ loop PT_LOAD segments
    │       ├─ copy p_filesz bytes → mem[p_vaddr]
    │       ├─ zero (p_memsz - p_filesz) bytes → BSS
    │       ├─ identify .text (PF_X) and .data
    │       └─ track highest end address
    │
    ├─ heap start = align16(highest end address)
    │
    ├─ parse sections ───────────────────────────────────────┐
    │       ├─ read all Section Headers                      │
    │       ├─ find shstrtab (section names)                 │
    │       ├─ find .symtab → iterate symbols                │
    │       │       └─ filter FUNC/OBJECT → symbol table     │
    │       └─ collect .data/.rodata/.bss → section list     │
    │                                                         │
    └─ ready to run ◄─────────────────────────────────────────┘
            ├─ entry, code base, code bytes, data base
            ├─ total size, heap start
            ├─ symbols  → labels (appear in disassembly)
            └─ sections → section list (side panel)
```

---

## 8. Inspecting an ELF yourself

You have the `hello-raven` binary at `hello-raven/target/riscv32im-unknown-none-elf/release/hello-raven`. To see what the loader will read:

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

The second segment has `FileSiz == MemSiz` (no BSS), flags `R E` (read + execute) → this is the `.text` identified as code.

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

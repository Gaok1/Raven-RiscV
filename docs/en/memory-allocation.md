# RAVEN — Dynamic Memory Allocation

In most environments you have `malloc` and `free`. In RAVEN there is no standard library — just raw RAM and a handful of syscalls. This guide walks through the three ways to allocate memory dynamically on the platform, from the simplest to the most general.

---

## Memory layout recap

```
0x00000000  ┌──────────────────────────────┐
            │  .text  (code)               │
            │                              │
0x00001000  ├──────────────────────────────┤
            │  .data  (initialized data)   │
            │  .bss   (zero-initialized)   │
            ├  ─  ─  ─  ─  ─  ─  ─  ─  ─ ┤ ← bss_end  (heap starts here)
            │                              │
            │        free space            │  ← heap grows ↑
            │                              │
            │        SAFE ZONE             │
            │                              │
            │        free space            │  ← stack grows ↓
            │                              │
0x0001FFFF  └──────────────────────────────┘
            sp (RAM_SIZE) - first push -> RAM_SIZE - 4
```

**RAM is configurable** (`16 MiB` by default in the UI/CLI). The heap and the stack share the free zone between `bss_end` and `RAM_SIZE`; manual bump allocators do not automatically detect a collision, so budget memory carefully.

---

## Why dynamic allocation?

Static data (`.data` / `.bss`) works well when sizes are known at compile time:

```asm
.bss
array: .space 400    ; exactly 100 words — fixed forever
```

Dynamic allocation is needed when the size is only known at runtime:

```asm
; user enters N, you need N * 4 bytes
; impossible to reserve this statically
```

---

## Approach 1 — Manual bump pointer

**No syscall required.** Keep a pointer in `.data` that tracks the current top of the heap and advance it on every allocation.

```asm
.data
heap_ptr: .word 0x00004000    ; initial heap base (choose above your .bss)

.text
; ─── alloc(a0 = size) → a0 = pointer to allocated block ───────────────────────
; Rounds size up to the next multiple of 4 (word alignment).
; Does NOT check for stack collision — caller must ensure there is room.
alloc:
    la   t0, heap_ptr
    lw   a0, 0(t0)            ; a0 = current heap_ptr  (will be returned)

    ; align size up: size = (size + 3) & ~3
    addi t1, a1, 3
    andi t1, t1, -4           ; t1 = aligned size

    add  t2, a0, t1           ; t2 = new heap_ptr
    sw   t2, 0(t0)            ; commit
    ret                       ; a0 = start of allocated block
```

### Usage

```asm
    li   a1, 20               ; request 20 bytes
    call alloc                ; a0 = pointer
    ; use a0 as a 20-byte buffer
```

### Pros / Cons

| | |
|---|---|
| **+** | Zero overhead, no syscall, trivial to understand |
| **+** | Works even in the earliest bootstrap code |
| **−** | No `free` — allocations are permanent |
| **−** | No OOM detection — silent corruption if heap meets stack |

---

## Approach 2 — `brk` syscall

`brk` lets the OS (Raven) manage the program break — the boundary between "used" and "free" heap. This is the foundation used by `malloc` implementations.

### Syscall reference

| Register | Value |
|----------|-------|
| `a7`     | `214` |
| `a0`     | new break address (pass `0` to query without changing) |
| **`a0` (ret)** | actual break after the call |

**Query:** `brk(0)` returns the current break without moving it.
**Extend:** `brk(addr)` tries to set the break to `addr`. If successful, returns `addr`. If Raven runs out of memory, it returns the *old* break (less than `addr`) — always check this.

### Emulating `sbrk(n)` in assembly

`sbrk(n)` is the classic "give me n more bytes" helper built on top of `brk`. Raven has no `sbrk` syscall; implement it yourself:

```asm
; ─── sbrk(a0 = bytes) → a0 = pointer to new block, or -1 on OOM ──────────────
sbrk:
    mv   t0, a0               ; save requested size

    ; step 1 — query current break
    li   a0, 0
    li   a7, 214
    ecall                     ; a0 = current break  (= start of new block)
    mv   t1, a0               ; t1 = old break (return value on success)

    ; step 2 — compute new break and request it
    add  a0, t1, t0           ; a0 = old_break + size
    li   a7, 214
    ecall                     ; a0 = actual new break

    ; step 3 — check: did Raven honour the request?
    add  t2, t1, t0           ; t2 = old_break + size  (what we wanted)
    blt  a0, t2, .sbrk_oom   ; if actual < wanted → OOM
    mv   a0, t1               ; return old break (start of allocated region)
    ret

.sbrk_oom:
    li   a0, -1               ; signal failure
    ret
```

### Usage

```asm
    li   a0, 256              ; request 256 bytes
    call sbrk
    li   t0, -1
    beq  a0, t0, out_of_memory
    ; a0 = pointer to 256-byte block
```

### Visualising `brk`

```
Before sbrk(256):                After sbrk(256):

    ┌────────────────┐               ┌────────────────┐
    │   .bss / data  │               │   .bss / data  │
    ├────────────────┤ ← old break   ├────────────────┤
    │                │               │  256 bytes     │ ← returned ptr
    │   free         │               ├────────────────┤ ← new break
    │                │               │                │
    │   stack ↓      │               │   free         │
    └────────────────┘               │                │
                                     │   stack ↓      │
                                     └────────────────┘
```

### Pros / Cons

| | |
|---|---|
| **+** | OOM is detectable (check return value) |
| **+** | No static `heap_ptr` — the OS tracks the break |
| **+** | Idiomatic — mirrors how real allocators work |
| **−** | No `free` — memory is only ever extended, never shrunk |
| **-** | Mixing `brk` and `mmap` in separate ad-hoc allocators can break allocator assumptions; prefer one strategy or one central allocator |

---

## Approach 3 — `mmap` anonymous

`mmap` allocates an anonymous block from the simulator-managed heap. In Raven only **anonymous** mappings are supported (no file-backed, no shared memory); internally it advances the same heap break used by `brk`.

### Syscall reference

| Register | Value |
|----------|-------|
| `a7`     | `222` |
| `a0`     | hint address — **ignored**, always pass `0` |
| `a1`     | length in bytes |
| `a2`     | prot — **ignored**, pass `3` (`PROT_READ\|PROT_WRITE`) |
| `a3`     | flags — must include `MAP_ANONYMOUS` (see below) |
| `a4`     | fd — **must be `-1`** for anonymous mappings |
| `a5`     | offset — **ignored**, pass `0` |
| **`a0` (ret)** | pointer to allocated block, or `-ENOMEM` / `-EINVAL` |

**Required flags:**

| Flag | Value | Meaning |
|------|-------|---------|
| `MAP_SHARED`    | `0x01` | (use MAP_PRIVATE instead) |
| `MAP_PRIVATE`   | `0x02` | mapping is private to this process |
| `MAP_ANONYMOUS` | `0x20` | no file backing |
| **Combined**    | **`0x22`** | `MAP_PRIVATE \| MAP_ANONYMOUS` — the standard value |

### Example — allocate a 512-byte buffer

```asm
    li   a0, 0          ; hint = 0 (ignored)
    li   a1, 512        ; length = 512 bytes
    li   a2, 3          ; PROT_READ|PROT_WRITE (ignored by Raven)
    li   a3, 0x22       ; MAP_PRIVATE|MAP_ANONYMOUS
    li   a4, -1         ; fd = -1
    li   a5, 0          ; offset = 0
    li   a7, 222
    ecall               ; a0 = pointer, or large negative value on failure

    ; check for error: mmap returns -ENOMEM (-12) or -EINVAL (-22) on failure
    blt  a0, zero, .mmap_error
    ; a0 is the usable pointer
    j    .mmap_ok
.mmap_error:
    ; handle error...
```

> **Checking for errors:** `mmap` error codes are returned as signed negative values
> (`-12` for OOM, `-22` for bad flags). A valid pointer is always positive
> on a 32-bit address space where the heap starts well above 0, so checking
> `blt a0, zero, error` is a safe heuristic.

### `munmap` — syscall 215

`munmap` is a **no-op** in Raven. Memory allocated with `mmap` (or `brk`) is **never freed**. Calling `munmap` returns `0` but has no effect.

```asm
    ; this does nothing in Raven — included only for API compatibility
    mv   a0, ptr
    li   a1, 512
    li   a7, 215
    ecall               ; always returns 0, memory not freed
```

### Pros / Cons

| | |
|---|---|
| **+** | Familiar API — same as Linux `mmap` |
| **+** | Each call returns the start of a newly reserved block |
| **+** | OOM is detectable (negative return value) |
| **−** | No `free` — `munmap` is a no-op |
| **-** | Internally uses the same heap break as `brk` - do not mix separate allocators unless they coordinate |

---

## Raven-specific limitations

| Limitation | Detail |
|---|---|
| **No `free`** | Neither `brk` nor `mmap` ever releases memory. Design programs to allocate once. |
| **`munmap` is a no-op** | Always returns 0; memory is not reclaimed. |
| **No `sbrk` syscall** | Emulate it with two `brk` calls (see Approach 2). |
| **`brk` and `mmap` share the same heap break** | They can coexist only if one allocator coordinates both. For simple assembly programs, pick one strategy. |
| **Configurable RAM** | Heap + stack must fit together within `RAM_SIZE` (default 16 MiB). A large heap leaves little room for deep call stacks. |
| **OOM = Raven says no** | If `brk` returns less than requested, or `mmap` returns a negative value, Raven has denied the allocation — you have hit the memory limit. |

---

## Comparison

| Feature | Bump pointer | `brk` (sbrk-style) | `mmap` anonymous |
|---|---|---|---|
| Syscall needed | No | Yes (214) | Yes (222) |
| Free memory | No | No | No (`munmap` = nop) |
| OOM detection | Manual (no guard) | Yes — check return value | Yes — check return value |
| Grows continuously | Yes | Yes | Per-block |
| Can mix with the other? | Manual only | Only with a coordinated allocator | Only with a coordinated allocator |
| Best for | Tiny programs, toy allocators | Growing a buffer step-by-step | Reserving fixed-size blocks through a Linux-like API |

---

## Quick reference

```
Syscall  Name      a0        a1     a2    a3      a4   a5    ret
───────  ────────  ────────  ─────  ────  ──────  ───  ───   ──────────────────
  214    brk       new_addr  —      —     —       —    —     actual break / old
  215    munmap    addr      len    —     —       —    —     0 (no-op)
  222    mmap      0(hint)   len    prot  0x22    -1   0     ptr / -errno
```

See also: [syscalls.md](syscalls.md) for the full syscall reference.

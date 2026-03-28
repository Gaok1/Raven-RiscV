# Hardware threads (harts)

> [Leia em Português](hart.pt-BR.md)

A **hart** — *hardware thread* — is an independent execution context within a RISC-V system: its own program counter, register file, and stack. In Raven, each hart maps to one simulated physical core. Multiple harts share the same flat address space and memory contents; they differ only in their register state and execution position.

---

## Multi-hart execution model

When a program spawns a hart, the simulator allocates a free core slot and schedules both harts in round-robin lockstep: each global tick advances every running hart by one step. Memory accesses from all harts go through the same cache hierarchy, so cache pressure from one hart is visible to all others.

Key properties:

- All harts share a single flat memory (no per-hart address spaces, no protection).
- Cache state (I-cache, D-cache, outer levels) is shared across all harts.
- Hart IDs are assigned by the simulator at spawn time and are unique across the session.
- A new hart becomes runnable on the next global cycle — never mid-cycle.
- The selected core in the UI determines which hart's register state and pipeline view is displayed.

---

## Lifecycle

| State   | Meaning                                                  |
|---------|----------------------------------------------------------|
| `FREE`  | Core slot not yet used (no hart assigned).               |
| `RUN`   | Hart is executing instructions normally.                 |
| `BRK`   | Hart hit a breakpoint or `ebreak`; paused for inspection.|
| `EXIT`  | Hart terminated via `hart_exit` or `exit_group`.         |
| `FAULT` | Hart encountered an unrecoverable error.                 |

A `FAULT` in any hart stops the entire simulation. An `EXIT` via `exit_group` (or `exit` syscall 93/94) also terminates all harts immediately. An `EXIT` via `hart_exit` (syscall 1101) terminates only the calling hart; the rest continue.

---

## Spawning harts from assembly

Use syscall `1100` (`hart_start`) to spawn a new hart.

**Registers on entry:**

| Register | Value                                |
|----------|--------------------------------------|
| `a7`     | `1100`                               |
| `a0`     | Entry PC — address of the first instruction the new hart will execute |
| `a1`     | Stack pointer — **top** (high address) of the new hart's stack region |
| `a2`     | Argument passed to the new hart in its `a0`                          |

**Return value:**

| `a0` | Meaning                              |
|------|--------------------------------------|
| ≥ 0  | Hart ID assigned by the simulator    |
| −1   | No free core slot available          |
| −2   | Entry PC is outside the loaded program |
| −3   | Stack pointer is invalid (zero, not 16-byte aligned, or outside memory) |

The new hart starts with a clean register file except for `sp` (set to `a1`) and `a0` (set to `a2`). All other registers begin at zero.

**Example (Falcon assembly):**

```asm
# Allocate a stack for the child hart (grows down from high address)
.data
hart1_stack: .space 4096
.text

main:
    la   a0, child_entry
    la   a1, hart1_stack
    addi a1, a1, 4096         # stack top = base + size
    li   a2, 42               # argument passed to child
    li   a7, 1100
    ecall                     # a0 = hart id (or < 0 on error)
    # ... main hart continues ...

child_entry:
    # a0 = argument (42)
    # ... do work ...
    li   a7, 1101
    ecall                     # hart_exit: only this hart terminates
```

---

## Terminating a hart

| Syscall | Number | Effect                                             |
|---------|--------|----------------------------------------------------|
| `hart_exit`   | `1101` | Terminate **this hart only**. Other harts keep running. |
| `exit`        | `93`   | Terminate **all harts**. Global program exit.           |
| `exit_group`  | `94`   | Terminate **all harts**. Identical to `exit` in Raven.  |

Use `hart_exit` from worker harts so the main hart continues. Use `exit` or `exit_group` only when the entire program should stop.

---

## Spawning harts from C (`c-to-raven`)

The `raven.h` header provides `falcon_hart_start` and the `SPAWN_HART` convenience macro.

```c
#include "raven.h"

static char worker_stack[4096];

void worker(unsigned int arg) {
    raven_print_uint(arg);
    falcon_hart_exit();
}

int main(void) {
    // Explicit call
    falcon_hart_start(
        (unsigned int)worker,
        (unsigned int)(worker_stack + sizeof(worker_stack)),
        /*arg=*/1
    );

    // Or with the convenience macro (stack array must be in scope)
    SPAWN_HART(worker, worker_stack, /*arg=*/2);

    raven_print_str("main done\n");
    return 0;
}
```

---

## Spawning harts from Rust (`rust-to-raven`)

Two variants are available in `raven_api::hart`:

### `spawn_hart_fn` — function pointer, zero allocation

```rust
use raven_api::{spawn_hart_fn, exit};

static mut STACK: [u8; 4096] = [0; 4096];

fn worker(id: u32) -> ! {
    // id is passed in a0 by the simulator
    raven_api::syscall::print_uint(id);
    raven_api::syscall::hart_exit()
}

fn main() {
    spawn_hart_fn(worker, unsafe { &mut STACK }, /*arg=*/1);
    // main continues...
    exit(0)
}
```

### `spawn_hart` — closure, heap-allocated

```rust
use raven_api::{spawn_hart, exit};

static mut STACK: [u8; 4096] = [0; 4096];

fn main() {
    let value = 99u32;
    spawn_hart(
        move || {
            raven_api::syscall::print_uint(value);
            raven_api::syscall::hart_exit()
        },
        unsafe { &mut STACK },
    );
    exit(0)
}
```

`spawn_hart` boxes the closure and passes a pointer to it in `a0` of the new hart. The trampoline function (`hart_trampoline`) unpacks and calls the closure, then calls `hart_exit`. The closure must never return normally; always call `hart_exit` (or `exit`) before it ends.

---

## UI — core selector

The Pipeline tab and the Run tab both show a **core selector** in the toolbar. Cycling through cores with the on-screen button (or keyboard shortcut) switches the register display, instruction view, and pipeline state to the selected hart without stopping execution.

Core status badges appear next to each core index: `RUN`, `BRK`, `EXIT`, `FAULT`, `FREE`.

The **Run scope** setting (Settings tab) controls whether `Run` (`Space`/`F5`) advances only the focused hart (`FOCUS`) or all running harts simultaneously (`ALL`). The `ALL` scope is the default for multi-hart programs.

---

## Stack requirements

Each hart needs its own stack. The requirements are:

- `stack_ptr` must be the **high address** (top) of the stack region — the stack grows down.
- `stack_ptr` must be **16-byte aligned** (RISC-V ABI requirement).
- `stack_ptr` must be within the loaded program's memory range (`0` to configured RAM size).
- The hart's stack must not overlap with any other hart's stack or the program's data segment.

Raven does **not** enforce stack bounds at runtime. Overflowing into another hart's stack or into the program data region produces undefined behavior that the simulator will not detect until an instruction faults.

---

## Configuring the number of cores

The maximum number of simultaneous harts is set in the **Settings tab** (`max_cores`). The default is `1`. Each additional core adds one slot that can be occupied by a spawned hart. Setting `max_cores` to `N` allows up to `N − 1` concurrent child harts plus the main hart.

Changes to `max_cores` take effect after the next program reset.

---

## See also

- [Syscall reference](syscalls.md) — full syscall table including `1100` and `1101`
- [Pipeline simulation](pipeline.md) — per-hart pipeline state and visualization
- [Memory map](syscalls.md#memory-map) — address layout used by all harts

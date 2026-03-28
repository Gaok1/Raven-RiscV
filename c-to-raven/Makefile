# ── Compiler detection ────────────────────────────────────────────────────────
# We need a RISC-V cross-compiler. Detect automatically unless the user
# passes CC= on the command line.
#
# Override example:
#   make CC=riscv64-unknown-elf-gcc
#   make CC=riscv64-none-elf-gcc

ifeq ($(origin CC), default)
  # make's default CC is "cc" — detect a proper RISC-V compiler instead
  ifneq ($(shell command -v clang 2>/dev/null),)
    CC := clang
  else ifneq ($(shell command -v clang-18 2>/dev/null),)
    CC := clang-18
  else ifneq ($(shell command -v riscv64-unknown-elf-gcc 2>/dev/null),)
    CC := riscv64-unknown-elf-gcc
  else ifneq ($(shell command -v riscv64-none-elf-gcc 2>/dev/null),)
    CC := riscv64-none-elf-gcc
  else
    $(error No RISC-V cross-compiler found. Install clang or riscv64-unknown-elf-gcc.)
  endif
endif

# ── Flags ────────────────────────────────────────────────────────────────────
# -fno-builtin: prevent the compiler from replacing loops with calls to
# libc memset/memcpy/etc., which would fail to link without a C library.
COMMON = -nostdlib -O2 -Wall -Wextra -fno-builtin

ifeq ($(findstring clang,$(CC)),clang)
  # Clang needs an explicit target triple + LLD linker
  CFLAGS_IM  = --target=riscv32-unknown-elf -march=rv32im  -mabi=ilp32  $(COMMON) -fuse-ld=lld
  CFLAGS_IMF = --target=riscv32-unknown-elf -march=rv32imf -mabi=ilp32f $(COMMON) -fuse-ld=lld
else
  # GCC cross-compiler already targets RISC-V
  CFLAGS_IM  = -march=rv32im  -mabi=ilp32  $(COMMON)
  CFLAGS_IMF = -march=rv32imf -mabi=ilp32f $(COMMON)
endif

LFLAGS = -e _start -Wl,--gc-sections

# ── Targets ───────────────────────────────────────────────────────────────────
TARGET       = c-to-raven.elf
FLOAT_TARGET = float-demo.elf
SRCS         = crt0.S main.c
FLOAT_SRCS   = crt0.S float_demo.c

.PHONY: all float-demo clean

all: $(TARGET)

$(TARGET): $(SRCS) raven.h internal/raven_internal.h internal/raven_syscall.h internal/raven_libc.h internal/raven_heap.h internal/raven_teaching.h
	$(CC) $(CFLAGS_IM) $(LFLAGS) -o $@ $(SRCS)

float-demo: $(FLOAT_TARGET)

$(FLOAT_TARGET): $(FLOAT_SRCS) raven.h internal/raven_internal.h internal/raven_syscall.h internal/raven_libc.h internal/raven_heap.h internal/raven_teaching.h
	$(CC) $(CFLAGS_IMF) $(LFLAGS) -o $@ $(FLOAT_SRCS)

clean:
	rm -f $(TARGET) $(FLOAT_TARGET)

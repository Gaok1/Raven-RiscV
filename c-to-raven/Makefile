# ─── Toolchain detection ─────────────────────────────────────────────────────
# Clang preferred (ships with LLD); fall back to a RISC-V cross-GCC.
ifeq ($(origin CC), default)
  ifneq ($(shell command -v clang 2>/dev/null),)
    CC := clang
  else ifneq ($(shell command -v clang-18 2>/dev/null),)
    CC := clang-18
  else ifneq ($(shell command -v riscv64-unknown-elf-gcc 2>/dev/null),)
    CC := riscv64-unknown-elf-gcc
  else ifneq ($(shell command -v riscv64-none-elf-gcc 2>/dev/null),)
    CC := riscv64-none-elf-gcc
  else
    $(error No RISC-V cross-compiler found. Install clang (preferred) or riscv64-unknown-elf-gcc.)
  endif
endif

# Archive tool: llvm-ar handles any target; GNU ar may not understand RV32 ELF.
ifeq ($(origin AR), default)
  ifneq ($(shell command -v llvm-ar 2>/dev/null),)
    AR := llvm-ar
  else
    AR := ar
  endif
endif

# ─── Flags ───────────────────────────────────────────────────────────────────
COMMON   = -nostdlib -O2 -Wall -Wextra -fno-builtin -ffreestanding
INCLUDES = -Iinclude

ifeq ($(findstring clang,$(CC)),clang)
  CFLAGS_IM  = --target=riscv32-unknown-elf -march=rv32im  -mabi=ilp32  $(COMMON) $(INCLUDES)
  CFLAGS_IMF = --target=riscv32-unknown-elf -march=rv32imf -mabi=ilp32f $(COMMON) $(INCLUDES)
  LFLAGS     = -e _start -Wl,--gc-sections -fuse-ld=lld
else
  CFLAGS_IM  = -march=rv32im  -mabi=ilp32  $(COMMON) $(INCLUDES)
  CFLAGS_IMF = -march=rv32imf -mabi=ilp32f $(COMMON) $(INCLUDES)
  LFLAGS     = -e _start -Wl,--gc-sections
endif

# ─── libraven.a (two ABI flavors: ilp32 soft-float, ilp32f hard-float) ───────
LIB_SRCS     = lib/io.c lib/fmt.c lib/mem.c lib/str.c lib/rand.c lib/hart.c lib/debug.c lib/coro.c
LIB_OBJS_IM  = $(LIB_SRCS:.c=.im.o)
LIB_OBJS_IMF = $(LIB_SRCS:.c=.imf.o)
TARGET_DIR   = target
USER_SRC     = src/main.c
USER_TARGET  = $(TARGET_DIR)/main.elf

LIB_IM  = lib/libraven.a
LIB_IMF = lib/libraven_f.a

CRT0_OBJ_IM  = lib/crt0.im.o
CRT0_OBJ_IMF = lib/crt0.imf.o

# Hand-written assembly that ships inside the archives (coroutine switch).
CORO_OBJ_IM  = lib/coro_switch.im.o
CORO_OBJ_IMF = lib/coro_switch.imf.o

# ─── Examples ────────────────────────────────────────────────────────────────
EXAMPLES = $(TARGET_DIR)/array_bench.elf $(TARGET_DIR)/jit_demo.elf $(TARGET_DIR)/float-demo.elf $(TARGET_DIR)/fmt_demo.elf $(TARGET_DIR)/coro_demo.elf

.PHONY: all examples lib clean
.SECONDARY:                # don't delete intermediate .o files

all: $(USER_TARGET) examples
lib: $(LIB_IM) $(LIB_IMF) $(CRT0_OBJ_IM) $(CRT0_OBJ_IMF) $(CORO_OBJ_IM) $(CORO_OBJ_IMF)
examples: $(EXAMPLES)

$(TARGET_DIR):
	mkdir -p $@

# Per-ABI object files
lib/%.im.o:  lib/%.c
	$(CC) $(CFLAGS_IM)  -c -o $@ $<

lib/%.imf.o: lib/%.c
	$(CC) $(CFLAGS_IMF) -c -o $@ $<

lib/crt0.im.o:  lib/crt0.S
	$(CC) $(CFLAGS_IM)  -c -o $@ $<

lib/crt0.imf.o: lib/crt0.S
	$(CC) $(CFLAGS_IMF) -c -o $@ $<

lib/coro_switch.im.o:  lib/coro_switch.S
	$(CC) $(CFLAGS_IM)  -c -o $@ $<

lib/coro_switch.imf.o: lib/coro_switch.S
	$(CC) $(CFLAGS_IMF) -c -o $@ $<

# Static archives
$(LIB_IM):  $(LIB_OBJS_IM)  $(CORO_OBJ_IM)
	$(AR) rcs $@ $^

$(LIB_IMF): $(LIB_OBJS_IMF) $(CORO_OBJ_IMF)
	$(AR) rcs $@ $^

# ─── User program: src/main.c -> target/main.elf ─────────────────────────────
$(USER_TARGET): $(USER_SRC) $(LIB_IMF) $(CRT0_OBJ_IMF) | $(TARGET_DIR)
	$(CC) $(CFLAGS_IMF) $(LFLAGS) -o $@ $< $(CRT0_OBJ_IMF) -Llib -lraven_f

# ─── Examples: rv32im (ilp32, soft float) ────────────────────────────────────
$(TARGET_DIR)/array_bench.elf: examples/array_bench.c $(LIB_IM) $(CRT0_OBJ_IM) | $(TARGET_DIR)
	$(CC) $(CFLAGS_IM) $(LFLAGS) -o $@ $< $(CRT0_OBJ_IM) -Llib -lraven

$(TARGET_DIR)/jit_demo.elf: examples/jit_demo.c $(LIB_IM) $(CRT0_OBJ_IM) | $(TARGET_DIR)
	$(CC) $(CFLAGS_IM) $(LFLAGS) -o $@ $< $(CRT0_OBJ_IM) -Llib -lraven

$(TARGET_DIR)/fmt_demo.elf: examples/fmt_demo.c $(LIB_IM) $(CRT0_OBJ_IM) | $(TARGET_DIR)
	$(CC) $(CFLAGS_IM) $(LFLAGS) -o $@ $< $(CRT0_OBJ_IM) -Llib -lraven

$(TARGET_DIR)/coro_demo.elf: examples/coro_demo.c $(LIB_IM) $(CRT0_OBJ_IM) | $(TARGET_DIR)
	$(CC) $(CFLAGS_IM) $(LFLAGS) -o $@ $< $(CRT0_OBJ_IM) -Llib -lraven

# ─── User programs: rv32imf (ilp32f, hardware float) ─────────────────────────
$(TARGET_DIR)/float-demo.elf: examples/float_demo.c $(LIB_IMF) $(CRT0_OBJ_IMF) | $(TARGET_DIR)
	$(CC) $(CFLAGS_IMF) $(LFLAGS) -o $@ $< $(CRT0_OBJ_IMF) -Llib -lraven_f

clean:
	rm -f $(LIB_IM) $(LIB_IMF) $(LIB_OBJS_IM) $(LIB_OBJS_IMF)
	rm -f $(CRT0_OBJ_IM) $(CRT0_OBJ_IMF) $(CORO_OBJ_IM) $(CORO_OBJ_IMF) $(USER_TARGET) $(EXAMPLES)
	rm -rf $(TARGET_DIR)

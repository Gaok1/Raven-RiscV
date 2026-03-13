TARGET := riscv32im-unknown-none-elf
BIN    := rust-to-raven

.PHONY: build release clean

build:
	cargo build
	cp target/$(TARGET)/debug/$(BIN) $(BIN).elf

release:
	cargo build --release
	cp target/$(TARGET)/release/$(BIN) $(BIN).elf

clean:
	cargo clean
	rm -f $(BIN).elf

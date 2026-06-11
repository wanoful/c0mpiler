.PHONY: run

run:
	@cargo run -q --release -- --target rv64 --emit asm

build:
	cargo build --release
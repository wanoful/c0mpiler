.PHONY: run

run:
	@cargo run -q --offline --release -- --target rv64 --emit asm

build:
	cargo build --release --offline
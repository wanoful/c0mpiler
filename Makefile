.PHONY: run

run:
	@cargo run -q -- --target rv64 --emit asm

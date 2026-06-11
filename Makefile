.PHONY: run

run:
	@cargo run -q --offline -- --target rv64 --emit asm

#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC="$SCRIPT_DIR/builtin.c"

echo "Generating RV32 assembly..."
clang --target=riscv32-unknown-elf -S "$SRC" -O2 -o "$SCRIPT_DIR/builtin_rv32.s"
echo "  -> builtin/builtin_rv32.s"

echo "Generating RV64 assembly..."
clang --target=riscv64-unknown-elf -S "$SRC" -O2 -o "$SCRIPT_DIR/builtin_rv64.s"
echo "  -> builtin/builtin_rv64.s"

echo "Done."

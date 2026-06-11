#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC="$SCRIPT_DIR/builtin.c"

echo "Generating RV32 assembly..."
riscv64-linux-gnu-gcc -march=rv32imafdc -mabi=ilp32 -S "$SRC" -O2 -o /tmp/builtin_rv32_raw.s
grep -v '^\s*\.attribute' /tmp/builtin_rv32_raw.s | grep -v '^\s*\.option' | grep -v '^\s*\.file' > "$SCRIPT_DIR/builtin_rv32.s"
echo "  -> builtin/builtin_rv32.s"

echo "Generating RV64 assembly..."
riscv64-linux-gnu-gcc -march=rv64imafdc -mabi=lp64d -S "$SRC" -O2 -o /tmp/builtin_rv64_raw.s
grep -v '^\s*\.attribute' /tmp/builtin_rv64_raw.s | grep -v '^\s*\.option' | grep -v '^\s*\.file' > "$SCRIPT_DIR/builtin_rv64.s"
echo "  -> builtin/builtin_rv64.s"

echo "Done."

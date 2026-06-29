# c0mpiler

`c0mpiler` is a Rust compiler for the `.rx` language used by this repository's
test suites. It parses and type-checks `.rx` source, lowers it to the project's
IR, runs IR optimizations, and emits RISC-V assembly for RV32 or RV64.

## Repository Layout

- `src/lexer`, `src/ast`, `src/semantics`: frontend lexer, parser, and semantic analysis.
- `src/ir`: core IR, IR builders, layouts, and IR optimization passes.
- `src/irgen`: AST-to-IR lowering.
- `src/mir`: machine IR lowering, register allocation, RV32/RV64 backends, and assembly printing.
- `builtin`: runtime/builtin support used when linking tests.
- `tests`: Rust integration tests for IR and assembly execution.
- `testcases` and `RCompiler-Testcases`: local testcase suites.
- `scripts/perf_rv64_case.sh`: RV64 qemu/perf profiling helper.

## Prerequisites

Core development:

```sh
cargo --version
```

RV64 assembly execution and profiling:

```sh
riscv64-linux-gnu-gcc --version
qemu-riscv64 --version
perf --version
```

RV32 assembly tests use REIMU. Set `REIMU_PATH` if `reimu` is not on `PATH`:

```sh
export REIMU_PATH=/home/wano/workspace/REIMU/build/linux/x86_64/release/reimu
```

## Build

```sh
cargo build
cargo build --release
```

The release binary is written to:

```text
target/release/c0mpiler
```

## CLI Usage

Compile a source file to RV32 assembly:

```sh
cargo run -- path/to/file.rx
```

Compile to RV64 assembly:

```sh
cargo run -- --target rv64 path/to/file.rx
```

Emit optimized IR instead of assembly:

```sh
cargo run -- --emit ir path/to/file.rx
```

Read source from stdin:

```sh
cat path/to/file.rx | cargo run -- --target rv64
```

Suppress builtin assembly emitted on stderr:

```sh
cargo run -- --target rv64 --no-builtin path/to/file.rx
```

The CLI options are:

```text
--target rv32|rv64    target architecture, default: rv32
--emit ir|asm         output format, default: asm
--no-builtin          do not print builtin assembly on stderr
```

## Optimization Pipeline

The compiler runs `IRGenerator::opt_all()` before IR printing or assembly
lowering. The pipeline combines scalar promotion, simplification, value
numbering, memory cleanup, loop optimization, inlining, and final cleanup passes
over several rounds so that later passes can use opportunities exposed by
earlier ones.

Current IR optimization passes:

| Pass | Purpose |
| --- | --- |
| `merge_return` | Merge function exits into a simpler return shape. |
| `scalar_memcpy` | Replace small `llvm.memcpy.*` / `llvm.memmove.*` calls with scalar loads and stores. |
| `sroa` | Split aggregate allocas into field-level allocas when accesses are simple and static. |
| `mem2reg` | Promote stack slots to SSA values. |
| `algebraic_simplification` | Fold algebraic identities, simple comparisons, casts, phis, selects, and constant global loads. |
| `local_gvn` | Eliminate equivalent expressions within one basic block. |
| `global_gvn` | Eliminate equivalent expressions across dominated blocks using the dominator tree. |
| `local_memory` | Remove duplicate GEPs, forward local loads, and delete overwritten stores with conservative alias checks. |
| `sccp` | Run sparse conditional constant propagation. |
| `strength_reduction` | Rewrite power-of-two multiply/divide/remainder into shifts and masks when safe. |
| `cfg_simplify` | Simplify control-flow after constants and dead code expose empty or redundant blocks. |
| `dce` / `adce` | Remove ordinary and aggressively discovered dead code. |
| `function_inline` | Inline suitable functions, then rerun cleanup and scalar optimizations. |
| `licm` | Move loop-invariant instructions out of loops when legal. |

The optimization pipeline currently exposes 15 IR pass entry points:
`mem2reg`, `sroa`, `dce`, `adce`, `sccp`, `cfg_simplify`, `merge_return`,
`function_inline`, `algebraic_simplification`, `licm`, `scalar_memcpy`,
`local_memory`, `local_gvn`, `global_gvn`, and `strength_reduction`.

Machine-level and lowering optimizations:

| Optimization | Purpose |
| --- | --- |
| RV64 32-bit lowering | Use RV64 word operations for 32-bit arithmetic where possible. |
| RV64 W instructions | Model and emit `addw`, `subw`, `mulw`, `divw`, `remw`, `sllw`, `srlw`, `sraw`, and immediate variants. |
| Integer normalization | Mask or sign-extend narrow integer values only when required by the operation or ABI. |
| Sign-extension suppression | Detect already sign-extended RV64 i32 values and skip redundant extension sequences. |
| Constant arithmetic lowering | Lower simple constant multiply/divide/remainder cases into moves, negation, shifts, masks, or cheaper instruction sequences. |
| Inline small memcpy | Expand small `llvm.memcpy.*` calls directly in lowering; 4-byte copies use one word load/store. |
| Zero-offset address cleanup | Avoid generating address additions when a GEP or field offset is zero. |
| Fallthrough and peephole cleanup | Remove redundant fallthrough jumps and run target peephole cleanup before assembly printing. |
| Branch relaxation fixups | Keep branch relaxation correct after fallthrough removal changes block layout. |

Optimization support includes release-mode build/run targets, RV64 qemu/perf
profiling through `scripts/perf_rv64_case.sh`, generated RV32/RV64 builtin
assembly, and local optimization testcases under `testcases/opti-local`.

## Common Test Commands

Run all Rust tests:

```sh
cargo test
```

Run RV64 assembly execution tests through `qemu-riscv64`:

```sh
cargo test ir_1_rv64_asm -- --nocapture
```

Run RV32 assembly execution tests through REIMU:

```sh
REIMU_PATH=/home/wano/workspace/REIMU/build/linux/x86_64/release/reimu \
  cargo test ir_1_asm -- --nocapture
```

Run a small RV64 end-to-end smoke test:

```sh
cargo test e2e_rv64 -- --nocapture
```

## RV64 qemu/perf Profiling

Use `scripts/perf_rv64_case.sh` to build one testcase into a static RV64 ELF,
run it under `qemu-riscv64`, compare output with the expected `.out`, and collect
perf/stat/disassembly artifacts.

Basic run:

```sh
RUNS=3 scripts/perf_rv64_case.sh comprehensive21 RCompiler-Testcases/IR-1
```

Useful run with qemu perf map and sampled perf report:

```sh
OUT_DIR=target/perf-rv64/IR-1/comprehensive21-profile \
RUNS=3 \
PERF_RECORD=1 \
QEMU_PERFMAP_ENABLE=1 \
scripts/perf_rv64_case.sh comprehensive21 RCompiler-Testcases/IR-1
```

Important outputs:

- `<OUT_DIR>/perf.summary.csv`: parsed `perf stat` counters for every run.
- `<OUT_DIR>/perf.<n>.txt`: raw `perf stat` output.
- `<OUT_DIR>/perf.<n>.data`: `perf record` data, when `PERF_RECORD=1`.
- `<OUT_DIR>/perf.<n>.report.txt`: text `perf report`, when `PERF_RECORD=1`.
- `<OUT_DIR>/perf.<n>.jit-functions.csv`: guest/JIT function overhead summary, when `PERF_RECORD=1`.
- `<OUT_DIR>/<case>.s`: generated assembly.
- `<OUT_DIR>/<case>.disasm.txt`: linked ELF disassembly.
- `<OUT_DIR>/<case>.asm.static.txt`: static assembly instruction and function counts.
- `<OUT_DIR>/<case>.symbols.txt`, `<case>.size.txt`, `<case>.sections.txt`: ELF metadata.

The script supports these common overrides:

```sh
QEMU_RV64_PATH=/path/to/qemu-riscv64
RISCV64_GCC=/path/to/riscv64-linux-gnu-gcc
PERF=/path/to/perf
PERF_EVENTS=cycles,instructions,branches,branch-misses
PERF_STAT_EXTRA="-d -d -d"
PERF_RECORD=1
PERF_RECORD_EVENT=cycles:u
PERF_RECORD_FREQ=997
QEMU_PERFMAP_ENABLE=1
QEMU_JITDUMP_ENABLE=1
KEEP_GOING=1
```

For optimization work, compare multiple runs with the same script options before
committing. `perf record` and qemu perf maps are useful for finding guest
hotspots; plain `RUNS=N` `perf stat` is usually better for validating whether an
optimization improves wall-clock/cycle counts.

## Manual RV64 Build and Run

This is the explicit sequence used by the profiling script:

```sh
cargo build --release

riscv64-linux-gnu-gcc -O2 -c builtin/builtin.c -o prelude.o
target/release/c0mpiler --target rv64 --emit asm --no-builtin input.rx > input.s
riscv64-linux-gnu-gcc -c input.s -o input.o
riscv64-linux-gnu-gcc -static prelude.o input.o -o input.elf
qemu-riscv64 input.elf
```

If the testcase has an input file:

```sh
qemu-riscv64 input.elf < input.in
```

## Notes

- The compiler currently emits assembly directly; linking is handled by external
  RISC-V tools in tests and scripts.
- RV32 assembly tests require REIMU. RV64 assembly tests require a RISC-V GNU
  toolchain and qemu user-mode emulator.
- Generated profiling artifacts are written under `target/` and are not meant to
  be committed.

#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/perf_rv64_case.sh [CASE_NAME] [CASE_ROOT]

Build one .rx testcase to a static RV64 ELF, run it under qemu-riscv64,
and collect host-side perf stat output for the qemu process.

Defaults:
  CASE_NAME=comprehensive21
  CASE_ROOT=RCompiler-Testcases/IR-1

Environment overrides:
  QEMU_RV64_PATH          qemu binary, default: qemu-riscv64
  RISCV64_GCC            rv64 gcc, default: riscv64-linux-gnu-gcc
  PERF                   perf binary, default: perf
  PERF_EVENTS            perf -e list, default:
                         cycles,instructions,branches,branch-misses,cache-references,cache-misses
  TIME                   time binary for fallback, default: /usr/bin/time
  RUNS                   number of measured runs, default: 1
  OUT_DIR                output dir, default: target/perf-rv64/<CASE_ROOT_BASENAME>/<CASE_NAME>
  CARGO_PROFILE          cargo profile, default: release
  KEEP_GOING             continue after output mismatch when set to 1, default: 0

Outputs:
  <OUT_DIR>/<case>.s
  <OUT_DIR>/<case>.o
  <OUT_DIR>/<case>.elf
  <OUT_DIR>/stdout.<n>.txt
  <OUT_DIR>/perf.<n>.txt
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

case_name="${1:-comprehensive21}"
case_root="${2:-RCompiler-Testcases/IR-1}"
qemu="${QEMU_RV64_PATH:-qemu-riscv64}"
gcc="${RISCV64_GCC:-riscv64-linux-gnu-gcc}"
perf_bin="${PERF:-perf}"
perf_events="${PERF_EVENTS:-cycles,instructions,branches,branch-misses,cache-references,cache-misses}"
time_bin="${TIME:-/usr/bin/time}"
runs="${RUNS:-1}"
cargo_profile="${CARGO_PROFILE:-release}"
keep_going="${KEEP_GOING:-0}"

src="${case_root}/src/${case_name}/${case_name}.rx"
input="${case_root}/src/${case_name}/${case_name}.in"
expected="${case_root}/src/${case_name}/${case_name}.out"
case_root_base="$(basename "${case_root}")"
out_dir="${OUT_DIR:-target/perf-rv64/${case_root_base}/${case_name}}"

if [[ ! -f "${src}" ]]; then
  echo "error: source file not found: ${src}" >&2
  exit 2
fi

mkdir -p "${out_dir}"

asm="${out_dir}/${case_name}.s"
obj="${out_dir}/${case_name}.o"
elf="${out_dir}/${case_name}.elf"
prelude_o="${out_dir}/prelude.o"

echo "[build] builtin -> ${prelude_o}"
"${gcc}" -O2 -c builtin/builtin.c -o "${prelude_o}"

echo "[build] compiler (${cargo_profile})"
cargo build "--${cargo_profile}"

compiler="target/${cargo_profile}/c0mpiler"
if [[ ! -x "${compiler}" ]]; then
  echo "error: compiler binary not found: ${compiler}" >&2
  exit 3
fi

echo "[build] ${src} -> ${asm}"
"${compiler}" --target rv64 --emit asm --no-builtin "${src}" > "${asm}"

echo "[build] asm -> ${obj}"
"${gcc}" -c "${asm}" -o "${obj}"

echo "[build] link -> ${elf}"
"${gcc}" -static "${prelude_o}" "${obj}" -o "${elf}"

for run in $(seq 1 "${runs}"); do
  stdout_file="${out_dir}/stdout.${run}.txt"
  perf_file="${out_dir}/perf.${run}.txt"
  if command -v "${perf_bin}" >/dev/null 2>&1; then
    echo "[run ${run}/${runs}] perf stat -> ${perf_file}"
    if [[ -f "${input}" ]]; then
      "${perf_bin}" stat -x, -e "${perf_events}" -o "${perf_file}" -- \
        "${qemu}" "${elf}" < "${input}" > "${stdout_file}"
    else
      "${perf_bin}" stat -x, -e "${perf_events}" -o "${perf_file}" -- \
        "${qemu}" "${elf}" > "${stdout_file}"
    fi
  else
    echo "[run ${run}/${runs}] ${perf_bin} not found; using ${time_bin} -v -> ${perf_file}"
    if [[ -f "${input}" ]]; then
      "${time_bin}" -v -o "${perf_file}" "${qemu}" "${elf}" < "${input}" > "${stdout_file}"
    else
      "${time_bin}" -v -o "${perf_file}" "${qemu}" "${elf}" > "${stdout_file}"
    fi
  fi

  if [[ -f "${expected}" ]]; then
    if ! diff -q --strip-trailing-cr "${expected}" "${stdout_file}" >/dev/null; then
      echo "warning: output mismatch for run ${run}" >&2
      diff -u --strip-trailing-cr "${expected}" "${stdout_file}" | sed -n '1,80p' >&2 || true
      if [[ "${keep_going}" != "1" ]]; then
        exit 4
      fi
    fi
  fi
done

echo "[done] artifacts in ${out_dir}"

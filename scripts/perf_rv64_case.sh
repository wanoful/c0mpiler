#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/perf_rv64_case.sh [CASE_NAME] [CASE_ROOT]

Build one .rx testcase to a static RV64 ELF, run it under qemu-riscv64,
and collect host-side perf data for the qemu process plus static RV64
assembly/disassembly summaries.

Defaults:
  CASE_NAME=comprehensive21
  CASE_ROOT=RCompiler-Testcases/IR-1

Environment overrides:
  QEMU_RV64_PATH          qemu binary, default: qemu-riscv64
  RISCV64_GCC            rv64 gcc, default: riscv64-linux-gnu-gcc
  PERF                   perf binary, default: perf
  PERF_EVENTS            perf -e list, default:
                         task-clock,cycles,instructions,branches,branch-misses,
                         cache-references,cache-misses,context-switches,cpu-migrations,
                         page-faults
  PERF_STAT_EXTRA        extra perf stat args, default: -d -d -d
  PERF_RECORD            run perf record/report when set to 1, default: 0
  PERF_RECORD_EVENT      perf record event, default: cycles:u
  PERF_RECORD_FREQ       perf record frequency, default: 997
  PERF_RECORD_EXTRA      extra perf record args, default: empty
  PERF_REPORT_EXTRA      extra perf report args, default: empty
  QEMU_PERFMAP_ENABLE    pass -perfmap to qemu when set to 1, default: 0
  QEMU_JITDUMP_ENABLE    pass -jitdump to qemu when set to 1, default: 0
  QEMU_EXTRA_ARGS        extra qemu args inserted before ELF, default: empty
  TIME                   time binary for fallback, default: /usr/bin/time
  RUNS                   number of measured runs, default: 1
  OUT_DIR                output dir, default: target/perf-rv64/<CASE_ROOT_BASENAME>/<CASE_NAME>
  CARGO_PROFILE          cargo profile, default: release
  KEEP_GOING             continue after output mismatch when set to 1, default: 0

Outputs:
  <OUT_DIR>/<case>.s
  <OUT_DIR>/<case>.o
  <OUT_DIR>/<case>.elf
  <OUT_DIR>/<case>.disasm.txt
  <OUT_DIR>/<case>.symbols.txt
  <OUT_DIR>/<case>.size.txt
  <OUT_DIR>/<case>.sections.txt
  <OUT_DIR>/<case>.asm.static.txt
  <OUT_DIR>/stdout.<n>.txt
  <OUT_DIR>/perf.<n>.txt
  <OUT_DIR>/perf.summary.csv
  Optional when PERF_RECORD=1:
    <OUT_DIR>/perf.<n>.data
    <OUT_DIR>/perf.<n>.report.txt
    <OUT_DIR>/perf.<n>.jit-functions.csv
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
perf_events="${PERF_EVENTS:-task-clock,cycles,instructions,branches,branch-misses,cache-references,cache-misses,context-switches,cpu-migrations,page-faults}"
perf_stat_extra="${PERF_STAT_EXTRA:--d -d -d}"
perf_record="${PERF_RECORD:-0}"
perf_record_event="${PERF_RECORD_EVENT:-cycles:u}"
perf_record_freq="${PERF_RECORD_FREQ:-997}"
perf_record_extra="${PERF_RECORD_EXTRA:-}"
perf_report_extra="${PERF_REPORT_EXTRA:-}"
qemu_perfmap_enable="${QEMU_PERFMAP_ENABLE:-0}"
qemu_jitdump_enable="${QEMU_JITDUMP_ENABLE:-0}"
qemu_extra_args="${QEMU_EXTRA_ARGS:-}"
time_bin="${TIME:-/usr/bin/time}"
runs="${RUNS:-1}"
cargo_profile="${CARGO_PROFILE:-release}"
keep_going="${KEEP_GOING:-0}"
objdump="${RISCV64_OBJDUMP:-riscv64-linux-gnu-objdump}"
nm="${RISCV64_NM:-riscv64-linux-gnu-nm}"
size_bin="${RISCV64_SIZE:-riscv64-linux-gnu-size}"
readelf="${RISCV64_READELF:-riscv64-linux-gnu-readelf}"

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

qemu_args=()
if [[ "${qemu_perfmap_enable}" == "1" ]]; then
  qemu_args+=("-perfmap")
fi
if [[ "${qemu_jitdump_enable}" == "1" ]]; then
  qemu_args+=("-jitdump")
fi
if [[ -n "${qemu_extra_args}" ]]; then
  # shellcheck disable=SC2206
  qemu_args+=(${qemu_extra_args})
fi

write_static_reports() {
  local asm_file="$1"
  local elf_file="$2"
  local disasm_file="${out_dir}/${case_name}.disasm.txt"
  local symbols_file="${out_dir}/${case_name}.symbols.txt"
  local size_file="${out_dir}/${case_name}.size.txt"
  local sections_file="${out_dir}/${case_name}.sections.txt"
  local static_file="${out_dir}/${case_name}.asm.static.txt"

  echo "[analyze] disasm -> ${disasm_file}"
  "${objdump}" -drwC "${elf_file}" > "${disasm_file}"
  echo "[analyze] symbols -> ${symbols_file}"
  "${nm}" -n --size-sort "${elf_file}" > "${symbols_file}" || "${nm}" -n "${elf_file}" > "${symbols_file}"
  echo "[analyze] size -> ${size_file}"
  "${size_bin}" -A "${elf_file}" > "${size_file}"
  echo "[analyze] sections -> ${sections_file}"
  "${readelf}" -SW "${elf_file}" > "${sections_file}"

  echo "[analyze] static asm summary -> ${static_file}"
  python3 - "${asm_file}" "${static_file}" <<'PY'
from collections import Counter, defaultdict
from pathlib import Path
import re
import sys

asm = Path(sys.argv[1])
out = Path(sys.argv[2])
inst = Counter()
per_func = defaultdict(Counter)
current = "<unknown>"
label_re = re.compile(r"^([A-Za-z_.$][\w.$]*):$")
inst_re = re.compile(r"^\s*([A-Za-z.][\w.]*)\b")
with asm.open() as f:
    for raw in f:
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        m = label_re.match(line)
        if m:
            label = m.group(1)
            if not label.startswith(".L"):
                current = label
            continue
        if line.startswith("."):
            continue
        m = inst_re.match(raw)
        if not m:
            continue
        op = m.group(1)
        inst[op] += 1
        per_func[current][op] += 1

total = sum(inst.values())
with out.open("w") as w:
    w.write(f"total_instructions,{total}\n")
    w.write("\n[top_instructions]\n")
    for op, n in inst.most_common(80):
        w.write(f"{op},{n}\n")
    w.write("\n[functions_by_static_instruction_count]\n")
    ranked = sorted(per_func.items(), key=lambda kv: sum(kv[1].values()), reverse=True)
    for func, counts in ranked[:80]:
        w.write(f"{func},{sum(counts.values())}\n")
    w.write("\n[top_instructions_per_function]\n")
    for func, counts in ranked[:30]:
        top = " ".join(f"{op}:{n}" for op, n in counts.most_common(12))
        w.write(f"{func},{sum(counts.values())},{top}\n")
PY
}

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

write_static_reports "${asm}" "${elf}"

summary_csv="${out_dir}/perf.summary.csv"
printf 'run,event,value,unit,metric\n' > "${summary_csv}"

for run in $(seq 1 "${runs}"); do
  stdout_file="${out_dir}/stdout.${run}.txt"
  perf_file="${out_dir}/perf.${run}.txt"
  if command -v "${perf_bin}" >/dev/null 2>&1; then
    echo "[run ${run}/${runs}] perf stat -> ${perf_file}"
    if [[ -f "${input}" ]]; then
      # shellcheck disable=SC2086
      "${perf_bin}" stat -x, ${perf_stat_extra} -e "${perf_events}" -o "${perf_file}" -- \
        "${qemu}" "${qemu_args[@]}" "${elf}" < "${input}" > "${stdout_file}"
    else
      # shellcheck disable=SC2086
      "${perf_bin}" stat -x, ${perf_stat_extra} -e "${perf_events}" -o "${perf_file}" -- \
        "${qemu}" "${qemu_args[@]}" "${elf}" > "${stdout_file}"
    fi

    python3 - "${run}" "${perf_file}" "${summary_csv}" <<'PY'
from pathlib import Path
import csv
import sys

run, perf_file, summary = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
with perf_file.open() as f, summary.open("a", newline="") as out:
    writer = csv.writer(out)
    for row in csv.reader(f):
        if not row or row[0].startswith("#") or len(row) < 3:
            continue
        value = row[0].strip()
        unit = row[1].strip() if len(row) > 1 else ""
        event = row[2].strip() if len(row) > 2 else ""
        metric = row[5].strip() if len(row) > 5 else ""
        if value and event:
            writer.writerow([run, event, value, unit, metric])
PY

    if [[ "${perf_record}" == "1" ]]; then
      data_file="${out_dir}/perf.${run}.data"
      report_file="${out_dir}/perf.${run}.report.txt"
      record_stdout="${out_dir}/stdout.record.${run}.txt"
      echo "[run ${run}/${runs}] perf record -> ${data_file}"
      if [[ -f "${input}" ]]; then
        # shellcheck disable=SC2086
        "${perf_bin}" record -q -F "${perf_record_freq}" -e "${perf_record_event}" ${perf_record_extra} \
          -o "${data_file}" -- "${qemu}" "${qemu_args[@]}" "${elf}" < "${input}" > "${record_stdout}"
      else
        # shellcheck disable=SC2086
        "${perf_bin}" record -q -F "${perf_record_freq}" -e "${perf_record_event}" ${perf_record_extra} \
          -o "${data_file}" -- "${qemu}" "${qemu_args[@]}" "${elf}" > "${record_stdout}"
      fi
      echo "[run ${run}/${runs}] perf report -> ${report_file}"
      # shellcheck disable=SC2086
      "${perf_bin}" report --stdio --sort comm,dso,symbol -i "${data_file}" ${perf_report_extra} \
        > "${report_file}" || true
      python3 - "${report_file}" "${out_dir}/perf.${run}.jit-functions.csv" <<'PY'
from collections import Counter
from pathlib import Path
import csv
import re
import sys

report = Path(sys.argv[1])
out = Path(sys.argv[2])
samples = Counter()
symbols = Counter()
symbol_re = re.compile(r"\[\.\]\s+(\S+)")
with report.open(errors="replace") as f:
    for line in f:
        percent_match = re.match(r"^\s*([0-9]+(?:\.[0-9]+)?)%", line)
        symbol_match = symbol_re.search(line)
        if not percent_match or not symbol_match:
            continue
        pct = float(percent_match.group(1))
        symbol = symbol_match.group(1)
        symbols[symbol] += pct
        function = symbol.split("+", 1)[0]
        samples[function] += pct

with out.open("w", newline="") as f:
    writer = csv.writer(f)
    writer.writerow(["function", "overhead_percent"])
    for function, pct in samples.most_common():
        writer.writerow([function, f"{pct:.2f}"])
    writer.writerow([])
    writer.writerow(["symbol", "overhead_percent"])
    for symbol, pct in symbols.most_common():
        writer.writerow([symbol, f"{pct:.2f}"])
PY
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
echo "[hint] perf stat summary: ${summary_csv}"
if [[ "${perf_record}" != "1" ]]; then
  echo "[hint] set PERF_RECORD=1 to also write perf.data and perf report files"
fi

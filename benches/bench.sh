#!/usr/bin/env bash
# Benchmark: none vs hot vs full para cada workload
# Uso: bash benches/bench.sh [--runs N]
# Requer: cargo build --release --features jit (já feito antes de rodar)

set -euo pipefail

RAVEN="./target/release/raven"
RUNS=3
ASM_DIR="benches/asm"

while [[ $# -gt 0 ]]; do
    case $1 in
        --runs) RUNS="$2"; shift 2;;
        *) echo "opção desconhecida: $1"; exit 1;;
    esac
done

WORKLOADS=(
    "array_sum:${ASM_DIR}/array_sum.asm"
    "fibonacci:${ASM_DIR}/fibonacci.asm"
    "bubble_sort:${ASM_DIR}/bubble_sort.asm"
    "c_to_raven:c-to-raven/c-to-raven.elf"
    "array_bench_c:c-to-raven/array_bench.elf"
)
MODES=(none hot full)

# Mede tempo de execução (wall clock em ms) para um dado modo + arquivo
measure() {
    local mode="$1"
    local file="$2"
    local total_ms=0
    for (( r=0; r<RUNS; r++ )); do
        local start end elapsed
        start=$(date +%s%N)
        "$RAVEN" run "$file" --jit="$mode" --nout > /dev/null 2>&1
        end=$(date +%s%N)
        elapsed=$(( (end - start) / 1000000 ))
        total_ms=$(( total_ms + elapsed ))
    done
    echo $(( total_ms / RUNS ))
}

printf "\n%-14s %8s %8s %8s   %8s %8s\n" "workload" "none(ms)" "hot(ms)" "full(ms)" "hot/none" "full/none"
printf '%s\n' "----------------------------------------------------------------------"

for entry in "${WORKLOADS[@]}"; do
    name="${entry%%:*}"
    file="${entry##*:}"

    if [[ ! -f "$file" ]]; then
        printf "%-14s  [arquivo não encontrado: %s]\n" "$name" "$file"
        continue
    fi

    t_none=$(measure none "$file")
    t_hot=$(measure  hot  "$file")
    t_full=$(measure full "$file")

    speedup_hot=$(awk "BEGIN { printf \"%.2fx\", $t_none / ($t_hot + 0.001) }")
    speedup_full=$(awk "BEGIN { printf \"%.2fx\", $t_none / ($t_full + 0.001) }")

    printf "%-14s %8d %8d %8d   %8s %8s\n" \
        "$name" "$t_none" "$t_hot" "$t_full" "$speedup_hot" "$speedup_full"
done

printf '\n'

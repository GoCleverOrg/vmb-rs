#!/usr/bin/env bash
# Frozen benchmark harness for cargo-mutants perf experiments.
#
# Do NOT modify this file during experiments — it's the equivalent of
# autoresearch's prepare.py. The whole point is that every iteration is
# measured by the same harness on the same crate with the same assertions.
#
# Usage:
#     scripts/mutants-bench.sh [--label "description"]
#
# What it does:
#     1. Ensures warm state (one warmup run, discarded).
#     2. Runs N=5 measured runs, timing each with Perl Time::HiRes.
#     3. Parses cargo-mutants summary. Verifies correctness vector against
#        the expected-outcomes file. Any deviation = "crash".
#     4. Prints a --- delimited summary with warm_min_s, warm_median_s,
#        mutants, caught, missed, unviable, timeout.
#     5. Appends a row to mutants-perf.tsv (gitignored).
#
# The expected correctness vector is stored in scripts/mutants-bench.expected
# as a single line: `mutants caught missed unviable timeout`.
# It can be updated when a deliberate change (e.g. excluding unviable mutants
# via mutants.toml) changes the vector. The update itself is an experiment
# and must be logged.

set -euo pipefail

CRATE="vmb"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_DIR="${REPO_ROOT}/target/mutants/${CRATE}"
MUTANTS_OUT="${OUTPUT_DIR}/mutants.out"
EXPECTED_FILE="${REPO_ROOT}/scripts/mutants-bench.expected"
TSV_FILE="${REPO_ROOT}/mutants-perf.tsv"
RUNS=5
LABEL="${1:-unlabelled}"
if [[ "${LABEL}" == "--label" && $# -ge 2 ]]; then
    LABEL="$2"
fi

# Cargo-mutants invocation — kept minimal. Experiments override via env vars:
#   EXTRA_MUTANT_ARGS  (e.g. "--baseline=skip")
#   PRECOMPILE_CMD     (e.g. "cargo test --no-run -p mira-remote-upload")
# Any other env vars (RUSTC_WRAPPER, CARGO_TARGET_DIR, RUSTFLAGS, ...) are
# passed through the environment. The editable config file mutants-bench.config
# is sourced if present — this is how each experiment persists its knobs.
if [[ -f "${REPO_ROOT}/mutants-bench.config" ]]; then
    # shellcheck disable=SC1091
    source "${REPO_ROOT}/mutants-bench.config"
fi
EXTRA_MUTANT_ARGS="${EXTRA_MUTANT_ARGS:-}"
PRECOMPILE_CMD="${PRECOMPILE_CMD:-}"

mkdir -p "${REPO_ROOT}/target/mutants"

# High-resolution wall-clock via Perl (avoids BSD date %N limitation).
now() { perl -MTime::HiRes=time -e 'printf "%.3f\n", time()'; }

run_once() {
    local log="$1"
    if [[ -n "${PRECOMPILE_CMD}" ]]; then
        eval "${PRECOMPILE_CMD}" >/dev/null 2>&1 || true
    fi
    local t0 t1
    t0=$(now)
    # shellcheck disable=SC2086
    cargo mutants --package "${CRATE}" \
        --timeout=120 --in-place \
        --output "${OUTPUT_DIR}" \
        ${EXTRA_MUTANT_ARGS} >"${log}" 2>&1 || true
    t1=$(now)
    perl -e "printf \"%.3f\n\", ${t1} - ${t0}"
}

# Build outcome vector from mutants.out/ ground-truth files.
# (Not parsed from stdout — cargo-mutants uses "mutant" vs "mutants" depending
# on count, which is fragile to regex.)
parse_vector() {
    local caught=0 missed=0 unviable=0 timeout=0 mutants=0
    if [[ -d "${MUTANTS_OUT}" ]]; then
        caught=$(wc -l <"${MUTANTS_OUT}/caught.txt" 2>/dev/null | tr -d ' ' || echo 0)
        missed=$(wc -l <"${MUTANTS_OUT}/missed.txt" 2>/dev/null | tr -d ' ' || echo 0)
        unviable=$(wc -l <"${MUTANTS_OUT}/unviable.txt" 2>/dev/null | tr -d ' ' || echo 0)
        timeout=$(wc -l <"${MUTANTS_OUT}/timeout.txt" 2>/dev/null | tr -d ' ' || echo 0)
    fi
    mutants=$((caught + missed + unviable + timeout))
    printf "%s %s %s %s %s\n" "${mutants}" "${caught}" "${missed}" "${unviable}" "${timeout}"
}

# ---- Warmup (priming compile cache, result discarded) -----------------------
echo "== warmup =="
WARMUP_LOG=$(mktemp)
run_once "${WARMUP_LOG}" >/dev/null
rm -f "${WARMUP_LOG}"

# ---- Measured runs ----------------------------------------------------------
echo "== measuring ${RUNS} runs =="
SAMPLES=()
VECTOR=""
for i in $(seq 1 "${RUNS}"); do
    LOG=$(mktemp)
    SECS=$(run_once "${LOG}")
    V=$(parse_vector)
    if [[ -z "${VECTOR}" ]]; then
        VECTOR="${V}"
    elif [[ "${V}" != "${VECTOR}" ]]; then
        echo "!! outcome vector drift on run ${i}: got '${V}', expected '${VECTOR}'"
        tail -20 "${LOG}"
        exit 2
    fi
    echo "  run ${i}: ${SECS}s  vector=[${V}]"
    SAMPLES+=("${SECS}")
    rm -f "${LOG}"
done

# ---- Stats ------------------------------------------------------------------
read -r min median < <(
    printf '%s\n' "${SAMPLES[@]}" | sort -g | awk '
        { a[NR]=$1 }
        END {
            n=NR
            if (n==0) { print "0 0"; exit }
            min=a[1]
            if (n%2==1) median=a[(n+1)/2]
            else        median=(a[n/2]+a[n/2+1])/2
            printf "%.3f %.3f\n", min, median
        }'
)

# ---- Correctness gate -------------------------------------------------------
STATUS="keep"
if [[ -f "${EXPECTED_FILE}" ]]; then
    EXPECTED="$(cat "${EXPECTED_FILE}")"
    if [[ "${VECTOR}" != "${EXPECTED}" ]]; then
        echo "!! correctness drift: vector=[${VECTOR}] expected=[${EXPECTED}]"
        STATUS="crash"
    fi
else
    echo "(no expected file; recording current vector as provisional truth)"
    echo "${VECTOR}" >"${EXPECTED_FILE}"
fi

# ---- Summary ----------------------------------------------------------------
read -r mutants caught missed unviable timeout <<<"${VECTOR}"
commit=$(git rev-parse --short=7 HEAD)

echo "---"
echo "commit:       ${commit}"
echo "label:        ${LABEL}"
echo "warm_min_s:   ${min}"
echo "warm_median_s:${median}"
echo "mutants:      ${mutants}"
echo "caught:       ${caught}"
echo "missed:       ${missed}"
echo "unviable:     ${unviable}"
echo "timeout:      ${timeout}"
echo "status:       ${STATUS}"

# ---- TSV append -------------------------------------------------------------
if [[ ! -f "${TSV_FILE}" ]]; then
    printf "commit\twarm_min_s\twarm_median_s\tmutants\tcaught\tmissed\tunviable\ttimeout\tstatus\tdescription\n" >"${TSV_FILE}"
fi
printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" \
    "${commit}" "${min}" "${median}" \
    "${mutants}" "${caught}" "${missed}" "${unviable}" "${timeout}" \
    "${STATUS}" "${LABEL}" >>"${TSV_FILE}"

[[ "${STATUS}" == "crash" ]] && exit 3 || exit 0

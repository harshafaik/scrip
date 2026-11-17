#!/usr/bin/env bash
# bench_generation.sh — Generation throughput + scaling + memory cliff
#
# Measures:
#  - Transaction generation throughput (tx/s) at default scale
#  - Customer generation throughput (customers/s)
#  - Core scaling via RAYON_NUM_THREADS=1,2,4,8
#  - Peak memory (RSS) at customer_count=10K, 100K (1M skipped: needs 32GB+)
#  - Parquet output sizes + compression ratio
#
# Dependencies: Rust toolchain, /usr/bin/time (GNU time with -v)
# Guard: checks cargo, config files

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

N_RUNS=5
SEED=42

# ── Hardware info ──────────────────────────────────────────────────────
echo "==> Hardware"
echo "    CPU:     $(lscpu | grep 'Model name' | cut -d: -f2 | xargs)"
echo "    Cores:   $(nproc) logical"
echo "    RAM:     $(free -h | awk '/Mem:/ {print $2}')"
echo "    Disk:    $(df -h / | awk 'NR==2 {print $1, $2}')"
echo "    OS:      $(uname -sr)"
echo

# ── Guards ──────────────────────────────────────────────────────────────
command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not found"; exit 1; }
command -v /usr/bin/time >/dev/null 2>&1 || { echo "WARNING: /usr/bin/time not found — RSS measurements will be skipped (install time package)"; }
[ -f data/config/customer_config.yaml ] || { echo "ERROR: data/config/customer_config.yaml not found"; exit 1; }
[ -f data/references/ref_residential.parquet ] || { echo "ERROR: data/references/ref_residential.parquet not found — run cargo run --bin export_references first"; exit 1; }
[ -f data/references/ref_merchants.parquet ] || { echo "ERROR: data/references/ref_merchants.parquet not found — run cargo run --bin export_references first"; exit 1; }

# ── Build release ───────────────────────────────────────────────────────
echo "==> Building release binary..."
cargo build --release --bin generate 2>&1 | tail -1

# ── Helper: restore default config ──────────────────────────────────────
CUSTOMER_CONFIG="data/config/customer_config.yaml"
DEFAULT_COUNT=$(grep 'customer_count' "$CUSTOMER_CONFIG" | head -1 | awk '{print $2}')
DEFAULT_STREAMING_RATE=$(grep 'streaming_rate' "$CUSTOMER_CONFIG" | head -1 | awk '{print $2}')

restore_config() {
    # Restore customer_count in config
    sed -i "s|customer_count: .*|customer_count: ${DEFAULT_COUNT}|" "$CUSTOMER_CONFIG"
}

# ── Helper: set customer count ──────────────────────────────────────────
set_customer_count() {
    local count=$1
    sed -i "s|customer_count: .*|customer_count: ${count}|" "$CUSTOMER_CONFIG"
}

# ── Helper: benchmark one run ───────────────────────────────────────────
run_gen() {
    local label="$1"
    local threads="${2:-}"
    local env_vars=""
    [ -n "$threads" ] && env_vars="RAYON_NUM_THREADS=${threads}"

    # Clear output dir for fresh timing
    rm -rf data/output

    if command -v /usr/bin/time >/dev/null 2>&1; then
        /usr/bin/time -v -o /tmp/gen_time.txt env $env_vars cargo run --release --bin generate 2>&1 | tee /tmp/gen_output.txt
    else
        env $env_vars cargo run --release --bin generate 2>&1 | tee /tmp/gen_output.txt
    fi

    local wall_time=$(grep -oP 'Total generation time: \K[\d.]+' /tmp/gen_output.txt 2>/dev/null || echo "N/A")
    local tx_count=$(grep -oP 'Total transactions: \K[\d]+' /tmp/gen_output.txt 2>/dev/null || echo "0")
    local cust_time=$(grep -oP 'Customer generation took: \K[\d.]+' /tmp/gen_output.txt 2>/dev/null || echo "0")
    local max_rss="N/A"
    if [ -f /tmp/gen_time.txt ]; then
        max_rss=$(grep 'Maximum resident set size' /tmp/gen_time.txt | awk '{print $6}')
    fi

    # Parquet sizes
    local tx_size=0 cust_size=0 acct_size=0 card_size=0 fraud_size=0
    [ -f data/output/transactions.parquet ] && tx_size=$(stat --format=%s data/output/transactions.parquet)
    [ -f data/output/customers.parquet ] && cust_size=$(stat --format=%s data/output/customers.parquet)
    [ -f data/output/accounts.parquet ] && acct_size=$(stat --format=%s data/output/accounts.parquet)
    [ -f data/output/cards.parquet ] && card_size=$(stat --format=%s data/output/cards.parquet)
    [ -f data/output/fraud_metadata.parquet ] && fraud_size=$(stat --format=%s data/output/fraud_metadata.parquet)
    total_size=$((tx_size + cust_size + acct_size + card_size + fraud_size))

    echo "${label}|${wall_time}|${tx_count}|${cust_time}|${max_rss}|${total_size}|${tx_size}"
}

# ── 1. Scaling at different customer counts ─────────────────────────────
echo
echo "==> 1. Transaction Throughput vs Scale (default threads, N=${N_RUNS})"
echo

# Run at default scale (3400 customers, ~1M txn)
restore_config
echo "   Scale: ${DEFAULT_COUNT} customers (~1M txn)"
for i in $(seq 1 $N_RUNS); do
    echo -n "   Run ${i}/${N_RUNS}... "
    result=$(run_gen "default" "")
    echo "done"
    echo "${result}" >> /tmp/gen_results.csv
done

# Run at 10K customers (~3M txn)
set_customer_count 10000
echo "   Scale: 10,000 customers (~3M txn)"
for i in $(seq 1 $N_RUNS); do
    echo -n "   Run ${i}/${N_RUNS}... "
    result=$(run_gen "10K" "")
    echo "done"
    echo "${result}" >> /tmp/gen_results.csv
done

# ── 2. Core Scaling ─────────────────────────────────────────────────────
restore_config
echo
echo "==> 2. Core Scaling (RAYON_NUM_THREADS) at default scale"
echo

for threads in 1 2 4 8; do
    echo "   Threads: ${threads}"
    result=$(run_gen "threads=${threads}" "${threads}")
    echo "${result}" >> /tmp/gen_scaling.csv
done

# ── 3. Memory Cliff ─────────────────────────────────────────────────────
echo
echo "==> 3. Peak RSS at Various Scales"
echo "| Customers | Peak RSS (KB) | RSS (MB) |"
echo "| ---: | ---: | ---: |"

for count in 3400 10000 100000; do
    set_customer_count "$count"
    echo -n "   ${count} customers... "
    result=$(run_gen "${count}" "")
    rss_kb=$(echo "${result}" | cut -d'|' -f5)
    if [ "$rss_kb" != "N/A" ] && [ -n "$rss_kb" ]; then
        rss_mb=$((rss_kb / 1024))
        echo "| ${count} | ${rss_kb} | ${rss_mb} |"
    else
        echo "| ${count} | N/A | N/A |"
    fi
done

echo
echo "   ⚠️ 1,000,000 customers skipped — requires ~32+ GB RAM."
echo "   Run: CUSTOMER_COUNT=1000000 bash benchmarks/bench_generation.sh"
echo

# ── 4. Parquet Size Summary ─────────────────────────────────────────────
echo "==> 4. Parquet Output Sizes (default scale)"
echo
if [ -f /tmp/gen_results.csv ]; then
    echo "| Metric | Value |"
    echo "| :--- | :--- |"
    sizes=$(tail -1 /tmp/gen_results.csv)
    total=$(echo "$sizes" | cut -d'|' -f6)
    tx_only=$(echo "$sizes" | cut -d'|' -f7)
    tx_count=$(echo "$sizes" | cut -d'|' -f3)
    if [ "$tx_only" != "0" ] && [ "$tx_count" != "0" ]; then
        bytes_per_tx=$((tx_only / tx_count))
        echo "| Total Parquet size | $((total / 1024 / 1024)) MB |"
        echo "| Transactions Parquet | $((tx_only / 1024)) KB |"
        echo "| Bytes per transaction | ${bytes_per_tx} |"
    fi
fi

# ── 5. Summary Table ────────────────────────────────────────────────────
echo
echo "==> 5. Generation Throughput Summary"
echo
echo "| Customers | Transactions | Wall Time (s) | tx/s | Peak RSS (MB) | Parquet Size (MB) | Byte/txn |"
echo "| ---: | ---: | ---: | ---: | ---: | ---: | ---: |"
if [ -f /tmp/gen_results.csv ]; then
    while IFS='|' read -r label wall tx cust_time rss total_size tx_size; do
        [ -z "$wall" ] && continue
        tx_s="N/A"
        rss_mb="N/A"
        parquet_mb="N/A"
        bpt="N/A"
        if [ "$wall" != "N/A" ] && [ "$wall" != "0" ] && [ "$tx" != "0" ]; then
            tx_s=$(awk "BEGIN {printf \"%.0f\", ${tx} / ${wall}}")
        fi
        if [ "$rss" != "N/A" ]; then
            rss_mb=$((rss / 1024))
        fi
        if [ "$total_size" != "0" ]; then
            parquet_mb=$((total_size / 1024 / 1024))
        fi
        if [ "$tx_size" != "0" ] && [ "$tx" != "0" ]; then
            bpt=$((tx_size / tx))
        fi
        echo "| ${label} | ${tx} | ${wall} | ${tx_s} | ${rss_mb} | ${parquet_mb} | ${bpt} |"
    done < /tmp/gen_results.csv
fi

echo
echo "==> Core Scaling Table"
echo
echo "| RAYON_NUM_THREADS | Wall Time (s) | tx/s |"
echo "| ---: | ---: | ---: |"
if [ -f /tmp/gen_scaling.csv ]; then
    while IFS='|' read -r label wall tx cust_time rss total_size tx_size; do
        [ -z "$wall" ] && continue
        tx_s="N/A"
        [ "$wall" != "N/A" ] && [ "$wall" != "0" ] && [ "$tx" != "0" ] && tx_s=$(awk "BEGIN {printf \"%.0f\", ${tx} / ${wall}}")
        echo "| ${label} | ${wall} | ${tx_s} |"
    done < /tmp/gen_scaling.csv
fi

# Restore config
restore_config
echo
echo "Done. Config restored to customer_count=${DEFAULT_COUNT}"

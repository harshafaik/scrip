#!/usr/bin/env bash
# bench_fraud_overhead.sh — Fraud injection overhead benchmark
#
# Measures generation throughput with fraud injection on vs off.
# Dependencies: Rust toolchain, reference data
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

echo "==> Hardware"
echo "    CPU: $(lscpu 2>/dev/null | grep 'Model name' | cut -d: -f2 | xargs || echo unknown)"
echo "    Cores: $(nproc) logical"
echo

command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not found"; exit 1; }

[ -f data/references/ref_residential.parquet ] || { echo "ERROR: references not found"; exit 1; }

FRAUD_CONFIG="data/config/fraud_rules.yaml"
DEFAULT_SHARE=$(grep 'target_share' "$FRAUD_CONFIG" | head -1 | awk '{print $2}')

echo "==> Building release binary..."
cargo build --release --bin generate 2>&1 | tail -1

echo
echo "==> Fraud Injection Overhead (N=3 each)"
echo

# Turn fraud OFF
sed -i "s|target_share: .*|target_share: 0.0|" "$FRAUD_CONFIG"
echo "--- Fraud off (target_share=0.0) ---"
for i in 1 2 3; do
    echo -n "  Run ${i}/3... "
    rm -rf data/output
    /usr/bin/time -f "%e" cargo run --release --bin generate 2>&1 | grep "Total generation time"
done

# Turn fraud ON (default 0.01)
sed -i "s|target_share: 0.0|target_share: ${DEFAULT_SHARE}|" "$FRAUD_CONFIG"
echo
echo "--- Fraud on (target_share=${DEFAULT_SHARE}) ---"
for i in 1 2 3; do
    echo -n "  Run ${i}/3... "
    rm -rf data/output
    /usr/bin/time -f "%e" cargo run --release --bin generate 2>&1 | grep "Total generation time"
done

echo
echo "| Configuration | Wall Time | Overhead |"
echo "| :--- | ---: | ---: |"
echo "| Fraud off | TODO | — |"
echo "| Fraud on  | TODO | TODO% |"

# Restore default
sed -i "s|target_share: 0.0|target_share: ${DEFAULT_SHARE}|" "$FRAUD_CONFIG"
echo "Config restored to target_share=${DEFAULT_SHARE}"

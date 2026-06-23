#!/usr/bin/env bash
# Sweep the swipe LM context-bonus knobs against a trained model directory.
# Usage: tools/lm_sweep.sh [lm_dir]
set -euo pipefail
cd "$(dirname "$0")/../.."
LM_DIR="${1:-assets/lm}"

cargo build --release -p engine-core --example validate_lm_context >/dev/null 2>&1
BIN=target/release/examples/validate_lm_context

echo "== sweep against $LM_DIR =="
for noise in 0.03 0.045 0.06; do
  for alpha in 0.5 0.7 1.0; do
    for scale in 0.35 0.4 0.5 1.0; do
      line=$(KRY_LM_ALPHA=$alpha KRY_LM_SCALE=$scale "$BIN" "$LM_DIR" "$noise" 2>/dev/null \
        | grep -E "baseline|with trained" | tr '\n' ' ')
      base=$(echo "$line" | sed -E 's/.*baseline \(no LM\)  reading_top1 = ([0-9.]+)%.*/\1/')
      lm=$(echo "$line"   | sed -E 's/.*with trained LM   reading_top1 = ([0-9.]+)%.*/\1/')
      printf "noise=%-5s alpha=%-3s scale=%-3s  base=%-5s  lm=%-5s\n" "$noise" "$alpha" "$scale" "$base" "$lm"
    done
  done
done

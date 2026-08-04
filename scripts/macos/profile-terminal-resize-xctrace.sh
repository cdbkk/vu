#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="${1:-$ROOT_DIR/dist/xctrace}"
mkdir -p "$OUT_DIR"

CARGO_BIN="${CARGO_BIN:-$(command -v cargo || true)}"
if [[ -z "$CARGO_BIN" ]]; then
  echo "error: cargo not found in PATH. Set CARGO_BIN to an absolute cargo path." >&2
  exit 1
fi

VU_APP_BIN="${VU_APP_BIN:-$ROOT_DIR/target/debug/vu}"

echo "Building Vu debug binary for profiling..."
"$CARGO_BIN" build -p vu >/dev/null

if [[ ! -x "$VU_APP_BIN" ]]; then
  echo "error: expected built app binary at $VU_APP_BIN. Set VU_APP_BIN to the executable to profile." >&2
  exit 1
fi

TRACE_NAME="vu-terminal-resize-$(date +%Y%m%d-%H%M%S).trace"
TRACE_PATH="$OUT_DIR/$TRACE_NAME"

echo "Recording Time Profiler trace to:"
echo "  $TRACE_PATH"
echo
echo "Workflow:"
echo "  1. The built Vu binary will launch under xctrace."
echo "  2. Reproduce 'claude --resume' and the bad live resize gesture."
echo "  3. Stop recording with Ctrl+C in this terminal."
echo

export VU_GHOSTTY_PROFILE=1
export RUST_LOG="${RUST_LOG:-vu::perf=info,vu_ghostty::perf=info,vu=warn,vu_core=warn,vu_agent=warn}"

xcrun xctrace record \
  --template 'Time Profiler' \
  --output "$TRACE_PATH" \
  --launch -- \
  "$VU_APP_BIN"

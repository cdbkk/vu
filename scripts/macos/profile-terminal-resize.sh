#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

export VU_GHOSTTY_PROFILE=1
export RUST_LOG="${RUST_LOG:-vu::perf=info,vu_ghostty::perf=info,vu=warn,vu_core=warn,vu_agent=warn}"

echo "Profiling terminal host path with:"
echo "  VU_GHOSTTY_PROFILE=$VU_GHOSTTY_PROFILE"
echo "  RUST_LOG=$RUST_LOG"
echo
echo "Reproduce:"
echo "  1. Start a heavy TUI such as 'claude --resume'"
echo "  2. Drag-resize the window for 3-5 seconds"
echo "  3. Capture vu::perf and vu_ghostty::perf lines"
echo

cargo run -p vu "$@"

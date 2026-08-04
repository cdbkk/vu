#!/bin/sh
# ponytail: zig 0.15.2 cannot parse the macOS 26 SDK's libSystem.tbd, so every
# link (including zig's own build runner) fails with "undefined symbol: _malloc".
# Zig resolves the SDK by shelling out to `xcrun --show-sdk-path` and ignores
# SDKROOT, so shim `xcrun` on PATH for zig only and answer that one query with an
# older, parseable SDK. Everything else delegates to the real xcrun, and
# rustc/clang/cmake keep the real Xcode.
#
# Ceiling: pinned to whatever VU_ZIG_SDK resolves to. Delete this whole file
# once the pinned Ghostty revision builds under zig 0.16, which handles the new SDK.
#
# Usage: VU_ZIG_BIN=scripts/zig-macos26-shim.sh cargo build

set -e

ZIG="${VU_ZIG_REAL:-$(command -v zig)}"
[ -n "$ZIG" ] || { echo "zig-macos26-shim: no zig on PATH; set VU_ZIG_REAL" >&2; exit 1; }

SDK="${VU_ZIG_SDK:-/Library/Developer/CommandLineTools/SDKs/MacOSX15.4.sdk}"
if [ ! -d "$SDK" ]; then
    exec "$ZIG" "$@" # no older SDK to fall back to; let zig fail loudly
fi

SHIM_BIN="${TMPDIR:-/tmp}/vu-zig-shim-bin"
mkdir -p "$SHIM_BIN"
cat > "$SHIM_BIN/xcrun" <<EOF
#!/bin/sh
for a in "\$@"; do
    case "\$a" in --show-sdk-path) echo "$SDK"; exit 0 ;; esac
done
exec /usr/bin/xcrun "\$@"
EOF
chmod +x "$SHIM_BIN/xcrun"

PATH="$SHIM_BIN:$PATH" exec "$ZIG" "$@"

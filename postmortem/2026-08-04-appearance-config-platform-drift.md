# Appearance config platform drift

## What happened

Commit `77a974a` added a twelfth positional appearance argument for macOS and
its app callers without updating the Linux and Windows backend signatures.
macOS stayed green while both portable targets failed to compile.

## Root cause

Four cfg-selected backends duplicated the same long positional API. The new
value was also opaque Ghostty config text, which only the macOS backend can
interpret; Linux and Windows use `libghostty-vt` without Ghostty's config
parser.

## Fix applied

The shared boundary now takes one `vu_ghostty::AppearanceConfig` containing
structured `Tweaks`. macOS renders those tweaks to config text internally,
Windows and the stub accept the same type, and Linux consumes the values that
map to its GPUI StyledText renderer.

## What we learned

Cfg-selected implementations need one shared data contract rather than
parallel positional signatures. Backend-specific serialization belongs behind
that contract, where unsupported values remain visible as typed data instead
of silently disappearing in an opaque string.

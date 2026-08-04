# vu-test

Logic test framework for `vu-cli` — launches a real vu process, resets state between
test files, and drives the session via plain-text `.test` files.

Inspired by [sqllogictest](https://www.sqlite.org/sqllogictest/doc/trunk/about.wiki).

See [`docs/impl/vu-test.md`](../../docs/impl/vu-test.md) for the full implementation
guide.

## Quick start

```bash
# Build everything
cargo build -p vu -p vu-cli -p vu-test

# Run all tests (vu is launched automatically)
./target/debug/vu-test crates/vu-test/testdata/

# Run a single file
./target/debug/vu-test crates/vu-test/testdata/tabs/basic.test

# Baseline mode — write actual output as new expected values
./target/debug/vu-test --rewrite crates/vu-test/testdata/

# Stop on first failure
./target/debug/vu-test --fail-fast crates/vu-test/testdata/

# Verbose — show pass/skip per step
./target/debug/vu-test --verbose crates/vu-test/testdata/
```

## .test file format

```
# comment

vu-cli <arguments...>       # preferred
# Legacy: old tests may still use `cmd ...`
---- <mode>
expected output here
```

### Match modes

| Mode          | Behaviour |
|---------------|-----------|
| `contains`    | actual output contains the expected string (default) |
| `exact`       | full string equality (trailing whitespace trimmed per line) |
| `json-subset` | every key/value in expected JSON exists in actual JSON |
| `ok`          | only checks exit code == 0; expected block ignored |
| `error`       | checks exit code != 0; expected matched against stderr |

### Example

```
# tabs/basic.test

vu-cli --json tabs list  # at least one tab exists
---- json-subset
{"tabs":[{"index":1}]}

vu-cli tabs new
---- ok

vu-cli --json tabs list
---- contains
"index":2

vu-cli --json tabs close --tab 2
---- ok
```

## CLI reference

```
vu-test [OPTIONS] <PATHS>...

Arguments:
  <PATHS>...  .test files or directories (searched recursively)

Options:
  --vu <PATH>              Path to vu app binary (default: target/ sibling, then PATH)
                            Env override: VU
                            Binary name: vu on Unix, vu-app on Windows
  --vu-cli <PATH>          Path to vu-cli binary (default: target/ sibling, then PATH)
                            Env override: VU_CLI
  --socket <PATH>           Control endpoint for the launched vu process
                            (default: temp Unix socket on Unix, named pipe on Windows)
  --startup-timeout <SECS> Seconds to wait for vu to start (default: 30)
  --rewrite                 Rewrite expected blocks from actual output (baseline mode)
  --fail-fast               Stop after the first failing file
  --verbose                 Show pass/skip results per step
```

## State isolation

Before each `.test` file, vu-test resets vu to a known baseline:
- All tabs except tab 1 are closed
- All panes in tab 1 except the first are closed (via Ctrl-D)
- All pane-local surfaces in the surviving pane except the first are closed

Every test file starts with exactly 1 tab, 1 pane, and 1 surface.

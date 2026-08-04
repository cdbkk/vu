# vu-test: Logic Test Framework

## Overview

`vu-test` is a sqllogictest-inspired E2E test framework for vu. It launches a real
vu process, drives it via `vu-cli` over the local socket, and verifies output using
plain-text `.test` files that inline both the command and the expected output.

## Design decisions

### One process, serial execution, state reset between files

Each `vu-test` run launches exactly one vu process. Test files run serially. Before
each file, `reset_state()` closes all extra tabs and panes so every file starts from a
known baseline: one tab, one pane, fresh shell.

This is simpler and more reliable than per-file process isolation (which would require
a headless vu mode that doesn't exist yet) and avoids the complexity of parallel
execution against a shared process.

### .test file format

```
# comment

vu-cli <arguments...>       # preferred
# Legacy: old tests may still use `cmd ...`
---- <mode>
expected output here
```

The `----` line carries the match mode inline. A blank line or the next step directive ends the
expected block.

**Match modes:**

| Mode          | Behaviour |
|---------------|-----------|
| `contains`    | actual output contains the expected string (default when `----` has no mode) |
| `exact`       | full string equality, trailing whitespace trimmed per line |
| `json-subset` | every key/value in expected JSON exists in actual JSON (ignores extra fields) |
| `ok`          | only checks exit code == 0; expected block ignored |
| `error`       | checks exit code != 0; expected block matched against stderr |
| `regex`       | not yet implemented |

**Example:**

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

### json-subset semantics

`json-subset` recursively checks that every key/value in the expected JSON exists in
the actual JSON. Extra keys in actual are ignored. Arrays: every element in expected
must appear somewhere in actual (order-independent).

This is the right default for most vu-cli assertions because the JSON responses
contain many fields (pane state, capabilities, etc.) that vary between runs.

### State reset

`reset_state()` runs before each `.test` file:

1. `reset_tabs` — closes all tabs with index > 1, one at a time, re-querying after
   each close so indices stay accurate.
2. `reset_panes` — sends Ctrl-D to extra panes in tab 1 until only one remains,
   keeping the pane with the lowest `pane_id`.
3. `reset_surfaces` — closes extra pane-local surfaces in the surviving pane,
   keeping the surface with the lowest `surface_id`.

### Binary resolution

The vu app binary and `vu-cli` are resolved in this order:

1. `--vu` / `--vu-cli` flag
2. `VU` / `VU_CLI` environment variable
3. Sibling binary in the same `target/` directory as `vu-test`
4. `PATH`

The default app binary name is platform-aware: `vu` on Unix and the retained
`vu-app` release alias on Windows. When running from the
workspace (`cargo build && ./target/debug/vu-test ...`), step 3 picks up the
freshly built binaries automatically.

### Control endpoint

If `--socket` is not provided, `vu-test` creates an isolated endpoint name for
the launched app:

- Unix: a temp Unix socket path like `/tmp/vu-test-<pid>.sock`
- Windows: a named pipe path like `\\.\pipe\vu-test-<pid>`

Readiness is detected by attempting to connect to the endpoint, not by checking
for a filesystem path. This keeps startup detection portable across Unix sockets
and Windows named pipes.

## Repository layout

```
crates/vu-test/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs      — CLI entry point, binary resolution, file collection, result printing
│   ├── parser.rs    — .test file parser, MatchMode, shell_split
│   └── runner.rs    — VuProcess RAII guard, reset_state, run_file, step execution
└── testdata/
    ├── system/
    │   ├── identify.test   — system.identify and capabilities smoke tests
    │   └── tree.test       — workspace tree structure
    ├── tabs/
    │   ├── basic.test      — tab list / new / close lifecycle
    │   └── rename.test     — tab user_label field assertions
    └── panes/
        ├── basic.test      — pane list field assertions
        ├── exec.test       — pane read and capability checks
        └── split.test      — pane create (split right) and layout
```

## Running locally

```bash
# Build everything
cargo build -p vu -p vu-cli -p vu-test

# Run all tests (vu launched automatically)
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

## CI

The GitHub Actions workflow at `.github/workflows/e2e.yml` runs on `macos-15` for
every push or PR that touches vu-test, vu-app, vu-core, vu-cli, or vu-ghostty.

Steps:
1. Build `vu`, `vu-cli`, `vu-test`
2. Run `cargo test -p vu-test` (parser unit tests, no live process needed)
3. Run `./target/debug/vu-test crates/vu-test/testdata/`
4. Upload `/tmp/vu-e2e.log` as an artifact on failure

## Known limitations

- `panes exec` requires shell integration (`exec_visible_shell` capability) which is
  not available immediately after vu starts. Tests that need exec should wait for
  shell integration or use `panes send-keys` + `panes read` instead.
- `panes wait` reads terminal scrollback which may contain output from previous
  sessions if vu restored terminal text. Tests should use unique sentinel strings
  (e.g. `VU_TEST_<name>_OK`) to avoid false matches.
- No variable support in `.test` files — `pane_id` and other dynamic values cannot
  be captured and reused across steps. Use `--pane-id` only when the ID is stable
  (e.g. always 0 for the first pane), or omit it to target the active pane.
- Parallel execution is not supported. All files run serially against one vu process.

## Adding tests

1. Create a `.test` file under `testdata/` in the appropriate subdirectory.
2. Write `vu-cli` / `---- <mode>` / expected blocks.
3. If unsure about exact output, run with `--rewrite` to generate the baseline:
   ```bash
   ./target/debug/vu-test --rewrite crates/vu-test/testdata/my-new-test.test
   ```
4. Review the diff, commit both the `.test` file and the generated expected values.

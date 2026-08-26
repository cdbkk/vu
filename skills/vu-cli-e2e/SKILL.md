---
name: vu-cli-e2e
description: Validate Vu's local socket control plane against a real running app session, and write or run vu-test integration tests. Use when testing vu-cli, the Unix socket API, pane control, tmux control, or E2E cases in crates/vu-test/testdata/.
---

# vu-cli E2E & vu-test

Use this skill when the task is to verify that Vu's CLI/control plane works against a live app window, or when writing/fixing integration tests in `crates/vu-test/testdata/`.

Primary reference:

- Read [`docs/impl/vu-cli-e2e.md`](../../docs/impl/vu-cli-e2e.md) for the full workflow and current live limitations.

---

## vu-cli manual E2E

Default workflow:

1. Build the relevant crates.
2. Launch `cargo run -p vu`.
3. Wait for `/tmp/vu.sock`.
4. Use `vu-cli --json identify`, `tabs list`, and `panes list` before acting.
5. Only use `panes exec` on panes that expose `exec_visible_shell`.
6. Use `tree` / `surfaces list` only for pane-local surface validation.
7. After `surfaces create` or `surfaces split`, use `surfaces wait-ready --surface-id <id> --timeout 10` before sending input that assumes an initialized shell.

Rules:

- Prefer `--json` for every command in automated evaluation.
- Prefer `pane_id` over `pane_index` for follow-up actions.
- Prefer `surface_id` for follow-up actions only when testing the explicit `surfaces.*` API.
- Keep existing pane tests on `panes.*`; surfaces are additive and must not change the pane model.
- After visible execution, confirm the pane still reports `shell_prompt` and keeps `exec_visible_shell`.

Known current limit:

- `panes create` now reports `surface_ready`, `is_alive`, and `has_shell_integration`, but startup-command panes can still be in a non-shell foreground state immediately after creation. Treat them as provisional until `panes list` confirms the capabilities you need for the next step.

---

## vu-test integration tests

`vu-test` is the E2E test runner for integration and interactive behavior. It launches a real vu session, runs `.test` files against it via `vu-cli`, and checks output.

### Running tests

```bash
# Build first
cargo build

# Run all tests
./target/debug/vu-test crates/vu-test/testdata/

# Run a single file
./target/debug/vu-test crates/vu-test/testdata/panes/split.test
```

### Test file format

Test files live in `crates/vu-test/testdata/<group>/<name>.test`. Each step is a `vu-cli` command followed by an assertion block:

```
# comment
vu-cli --json <command>   # step description
---- <assertion>
<expected>
```

Assertion types:

| Assertion | Meaning |
|---|---|
| `---- ok` | Command exits 0 (any output accepted) |
| `---- contains` | stdout contains the literal string on the next line |
| `---- json-subset` | actual JSON is a superset of the expected JSON (subset match, deep) |

The `json-subset` assertion only checks the keys you specify — extra fields in the actual output are ignored. Use it to assert specific fields without coupling to the full response shape.

### Writing new tests

- **Unit-test functions** in Rust (`#[cfg(test)]`) for logic. Use `vu-test` only for integration and interactive behavior that requires a live session.
- **No low-value tests** — don't write tests just to hit coverage. Every test should catch a real bug or document a real contract.
- Group tests by domain: `panes/`, `tabs/`, `surfaces/`, `system/`.
- After `panes create`, always add a `panes wait` step before asserting `is_alive` — the new pane's surface may not be ready immediately.

### Example test

```
# panes/split.test
vu-cli --json panes list --tab 1  # start with 1 pane
---- json-subset
{"panes":[{"index":1}]}

vu-cli --json panes create --tab 1 --location right  # split right
---- ok

vu-cli --json panes wait --tab 1 --pane-index 2 --timeout 5  # wait for new pane
---- ok

vu-cli --json panes list --tab 1  # both panes alive
---- json-subset
{"panes":[{"is_alive":true},{"is_alive":true}]}
```

### Fixing failures

When a test fails with `expected JSON is not a subset of actual JSON`, the actual output is printed in full. Check:

1. Is the command missing (unrecognized subcommand)? → Add the subcommand to `vu-cli` and the `ControlCommand` handler.
2. Is a field wrong/missing? → Fix the handler's response shape.
3. Is the assertion racing (e.g. `is_alive: false` right after create)? → Add a `panes wait` or `surfaces wait-ready` step before the assertion.

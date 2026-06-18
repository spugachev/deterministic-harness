---
name: verify
description: Run the full quick verification tier and interpret failures
disable-model-invocation: true
allowed-tools: Bash
---

# Quick verification tier

Runs inside the `dhx:latest` image (there is no host `dhx`); the cache volumes
keep re-runs fast:

!`docker run --rm -v "$PWD":/work -w /work -v dhx-cargo-registry:/root/.cargo/registry -v "dhx-target-$(basename "$PWD")":/work/target dhx:latest dhx verify --quick 2>&1 | tail -60`

**Passed:** ready to commit/push.
**Failed:** read the failing gate, fix the root cause, re-run `dhx check` then this.
Common: fmt → `cargo fmt`; clippy → fix or `#[allow(..., reason=...)]`;
mutation-coverage → add a mutations.toml entry; traceability → fix the link.

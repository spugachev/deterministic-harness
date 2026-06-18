# seats

A verified Rust service scaffolded by **Deterministic Harness** (`dhx`).

## Quickstart

Every tier runs inside the `dhx:latest` image (there is no host `dhx`). Define
this shell function once — the cache volumes make iteration fast (deps download
once, `target/` is reused; without them every run recompiles from scratch):

```sh
dhx() { docker run --rm -v "$PWD":/work -w /work \
          -v dhx-cargo-registry:/root/.cargo/registry \
          -v "dhx-target-$(basename "$PWD")":/work/target \
          dhx:latest dhx "$@"; }

dhx check            # every save: fmt, clippy, traceability, spec-sync, BDD coverage, …
dhx verify --quick   # after small changes: + tests, proptest, coverage, Kani, TLA+/TLC, DST
dhx verify --full    # after big changes / before release: + TSAN, mutants, fuzz, Loom, multi-seed DST
```

## Layout

- `crates/core` — the IO-free verified core (your pure domain + ports). Ships a
  throwaway `domain::example` (REQ-001) as a green seed — replace it.
- `spec/` — requirements (`REQ-NNN`), BDD features (EARS Gherkin), ADRs, and any
  TLA+ specs.
- `.harness/` — version pins, relocated tool configs, git hooks.
- `harness.toml` — the one manifest `dhx` reads.

See `CLAUDE.md` for the architecture precondition and the dev loop.

## Status

<!-- dhx:counts -->
12 Gherkin scenario(s) across 3 feature file(s); requirements up to REQ-006.
<!-- /dhx:counts -->

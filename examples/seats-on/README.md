# seats

A verified Rust service scaffolded by **Deterministic Harness** (`dhx`).

## Quickstart

```sh
dhx check            # cheap gates (fmt, clippy, traceability, spec-sync, …)
dhx verify --quick   # + tests, coverage, Kani, DST
dhx verify --full    # everything, in the pinned Docker image
```

## Layout

- `crates/core` — the IO-free verified core (domain FSM + ports).
- `spec/` — requirements (`REQ-NNN`), ADRs, TLA+ specs (+ `mutations.toml`).
- `.harness/` — version pins, relocated tool configs, git hooks.
- `harness.toml` — the one manifest `dhx` reads.

See `CLAUDE.md` for the architecture precondition and the dev loop.

## Status

<!-- dhx:counts -->
0 Gherkin scenario(s) across 0 feature file(s); requirements up to REQ-004.
<!-- /dhx:counts -->

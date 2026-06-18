# seats

A verified seat-reservation service scaffolded by **Deterministic Harness**
(`dhx`): clients hold seats for a single fixed-capacity event, confirm or release
holds, and holds expire after a TTL — with a proven no-overbooking invariant.

## Quickstart

Every tier runs inside the `dhx:latest` image (there is no host `dhx`); an alias
`dhx() { docker run --rm -v "$PWD":/work -w /work dhx:latest dhx "$@"; }` makes
these read like local commands:

```sh
dhx check            # every save: fmt, clippy, traceability, spec-sync, BDD coverage, …
dhx verify --quick   # after small changes: + tests, proptest, coverage, Kani, TLA+/TLC, DST
dhx verify --full    # after big changes / before release: + Miri, TSAN, mutants, fuzz, Loom
```

## Layout

- `crates/core` — the IO-free verified core: the hold lifecycle FSM
  (`domain::hold`), the reservation/capacity logic (`domain::reservation`), the
  Clock/Rng/IdGen ports, and the `#[cfg(kani)]` no-overbooking proofs.
- `crates/api` — the OUTER IO crate: the axum HTTP layer (`app`) and the
  production port adapters (`adapters`, where real wall-clock time enters), plus
  the HTTP and DST integration tests.
- `spec/` — requirements (`REQ-001..006`), BDD features (EARS Gherkin), ADRs, and
  the FSM-generated TLA+ lifecycle spec (`tla/Lifecycle.tla` + `mutations.toml`).
- `.harness/` — version pins, relocated tool configs, git hooks.
- `harness.toml` — the one manifest `dhx` reads (incl. the `[fsm]` section).

See `CLAUDE.md` for the architecture precondition and the dev loop.

## Status

<!-- dhx:counts -->
10 Gherkin scenario(s) across 1 feature file(s); requirements up to REQ-006.
<!-- /dhx:counts -->

# Architecture — the shape that gives the gates teeth

The gates in [toolchain.md](toolchain.md) are not generic. They verify a
*specific architecture*, and that architecture is what makes them meaningful.
`dhx init` lays it down so a fresh project is verifiable by construction.

## The precondition: an IO-free core behind ports

```
my-svc/
├── crates/core/            the VERIFIED CORE — pure, no IO, no async runtime
│   └── src/
│       ├── domain/         the types + the FSM (state.rs::next)
│       └── ports/          Clock, Rng, IdGen — the determinism seam
└── (outer crates)          HTTP / DB / CLI adapters live here, behind the ports
```

Two rules, and the reason for each:

1. **The core is IO-free.** No database, no HTTP, no tokio. A pure function has
   no hidden inputs, so Kani/Verus can prove it total and proptest can assert
   laws over it. The moment IO leaks into the core, those proofs lose their
   footing.
2. **All non-determinism flows through ports.** Domain code never calls
   `SystemTime::now`, `Instant::now`, or a thread RNG directly — it takes a
   `Clock`/`Rng`/`IdGen`. Production wires real adapters; tests and DST wire
   seeded/mocked ones. This is the seam that makes DST/Loom/TSAN *deterministic*
   and therefore replayable. `clippy.toml` bans the direct calls
   (`disallowed_methods`) so the discipline can't erode.

A persistence or HTTP layer (axum, sqlx, anything) is an **optional outer
crate** behind the ports — never a harness requirement. The harness needs the
*architecture*, not any specific library.

## The FSM (optional but first-class)

If the domain has a state machine, write it once as a pure `state.rs::next`
function. `dhx regen` projects it into `spec/tla/Lifecycle.{tla,cfg}` so the
TLA+ model checker verifies *the same transition table the code executes* — no
hand-maintained second copy to drift. If a project has no FSM, omit the `[fsm]`
section and those gates skip (but see "presence ⇒ mandatory" below).

## The project layout

Only the two files cargo/rustup force to fixed locations stay at the root;
everything the harness owns is centralized.

```
my-svc/
├── rust-toolchain.toml     root (rustup) — the STABLE channel pin
├── Cargo.toml              a workspace (the coverage-driving pattern needs it)
├── harness.toml            the ONE manifest dhx reads — see configuration.md
├── README.md               has a <!-- dhx:counts --> region dhx keeps honest
├── Dockerfile              the --full image, built from .harness/pins/*
├── .cargo/config.toml      forced here (cargo) — rustflags
├── clippy.toml             the determinism bans + complexity threshold
├── .harness/
│   ├── pins/               nightly, verus, tla2tools, dhx — the version authority
│   ├── config/             deny / gitleaks / mutants / nextest (relocated via flags)
│   └── hooks/              pre-commit + commit-msg (thin: they call dhx)
├── spec/
│   ├── requirements/       REQ-NNN.md (EARS acceptance criteria)
│   ├── adr/                ADR-NNNN.md (decisions)
│   └── tla/                *.tla / *.cfg + mutations.toml
└── crates/                 the workspace members
```

## Determinism: the sources, and how each is controlled

| Source of non-determinism | Controlled by |
|---|---|
| Wall-clock time | a `Clock` port (mocked in tests/DST); `SystemTime::now` banned in domain code |
| Randomness | an `Rng` port seeded per run; `thread_rng` banned |
| Id generation | an `IdGen` port (seeded/sequential in tests) |
| Concurrency / interleavings | Loom (exhaustive in-memory), TSAN (real threads), DST (seeded schedules) |
| Tool versions across machines | `.harness/pins/*`; `--full` runs in a Docker image built from them |
| Discovery entropy (fuzz / random DST) | the *finding* is persisted and replays deterministically |

## Presence ⇒ mandatory (the anti-toothless invariant)

The architecture is enforced at config-load, not trusted:

- If `crates/core/src/domain/state.rs` (an FSM *source*) exists but `harness.toml`
  has no `[fsm]` section → **load error**, not a silent skip.
- If `spec/tla/*.tla` declares invariants but `mutations.toml` doesn't cover them
  → `check-mutation-coverage` **fails**.
- If a REQ criterion claims `(verified=kani)` with no backing link →
  `check-verified-markers` **fails**.

The rule: an input that *looks* verifiable but isn't configured is an error.
"Out of scope" must be declared, never inferred from absence. See
[philosophy.md](philosophy.md#gates-with-teeth--and-the-worst-failure-mode).

## Why a workspace, and the coverage-driving pattern

The verified core's functions are often exercised mostly by *other* crates'
integration tests, not by the core's own unit tests. So `dhx cov` runs the
**whole** workspace test suite but restricts the coverage *report* to the
`[coverage].core` crate(s) — measuring the bar on the crate you care about while
every test that exercises it counts. That pattern needs a workspace; a
single-crate project is out of scope.

# seats — project conventions for Claude

This is a **Deterministic Harness** project: a verified Rust service scaffolded
by `dhx`. The verification toolchain is the whole quality story — there is no CI;
the `dhx` gates run locally and are the entire gate.

## Architecture precondition (why the gates have teeth)

All non-determinism flows through **ports** (`crates/core/src/ports/`:
Clock/Rng/IdGen). Domain/application code must NEVER call `SystemTime::now`,
`Instant::now`, or a thread RNG directly — `clippy.toml` bans them. This
discipline is what makes the concurrency/DST gates meaningful and the domain
reproducible. The **verified core** (`crates/core`) is IO-free so Kani/proptest
can prove its functions total. Add any IO — HTTP, DB, files, sockets — in OUTER
crates behind the ports, never in the core. This shape is independent of what
the project *does*; the shipped `domain::example` is only a green starter seed
to delete and replace with your domain.

## Layout

```
harness.toml          the one manifest dhx reads (gates, targets, pins, fsm)
.harness/             pins/, config/ (relocated tool configs), hooks/
spec/                 requirements/ (REQ-NNN), adr/, tla/ (+ mutations.toml)
crates/core/          the IO-free verified core: domain/ (the FSM), ports/
```

## The methodology — spec-first, from specification to code to simulation

This works for **any** kind of project (CLI, library, service, protocol) — the
scaffold ships a tiny `domain::example` (REQ-001) only as a green starting seed;
delete it and add your domain. The order is **spec → code → simulation**, NOT
test-first TDD:
pin the *intent* in a spec, derive the code from it, then prove it. Adequacy is
enforced by mutation testing + coverage, not by test-ordering.

**The mandatory floor — these run on EVERY feature, no exceptions:**

- **BDD + EARS (cucumber).** Every `REQ-NNN` gets at least one Gherkin scenario
  tagged with its id, phrased in EARS form (Given a state / When an event /
  Then the system shall …). `check-bdd-coverage` FAILS a REQ that has no
  scenario — there is no opt-out. Scenarios drive the domain directly (the core
  is IO-free, as the shipped `crates/core/tests/bdd.rs` shows); they do not need
  HTTP. A `(verified=…)` marker can satisfy *token-matching* for a criterion
  that is not externally observable, but it never replaces the scenario.
- **clippy** (4 levels + restriction) and **property tests (proptest)** for the
  pure laws of the feature (totality, idempotence, monotonicity, round-trip,
  bounds). These are the broad, cheap workhorses you always write.

**Per feature, work the phases in this order:**

1. **REQ first.** Write `spec/requirements/REQ-NNN.md` with EARS acceptance
   criteria. (`check-traceability` ties it to the code/spec that implements it.)
2. **Specify before coding — TLA+ and/or BDD:**
   - **BDD always** — write the EARS Gherkin scenarios for the REQ now; they are
     the executable acceptance spec you code against.
   - **TLA+ when the feature is concurrent / a protocol / has interleavings** —
     model it in `spec/tla/*.tla`, run `dhx tlc`. If the feature is a state
     machine, write it as a pure `fn next(state, event) -> Option<state>`, add an
     `[fsm]` section to `harness.toml`, and `dhx regen` *generates* the TLA+ from
     the Rust (edit Rust, regen, commit) — so you only hand-write TLA+ for
     genuinely concurrent specs. Every invariant needs an anti-vacuity entry in
     `spec/tla/mutations.toml` (`check-mutation-coverage`).
3. **Implement** the code to satisfy the spec — pure logic in `crates/core`, any
   IO in an outer crate behind a port.
4. **Unit + property tests.** Unit tests for the concrete cases; **proptest** for
   the laws. `dhx cov` holds the core at the `[coverage].core` bar; `dhx mutants`
   proves the tests actually *kill* logic mutations (a test green with the logic
   inverted is weak and fails the gate).
5. **DST** for any multi-step / stateful / network behaviour — drive the real
   thing over a simulated world with a seed; a failure replays deterministically.
6. **Route the rest by the feature's hardest question** (below) — reach for the
   specialised instruments only where they pay off.

**Routing rubric for the OPTIONAL instruments — do NOT run every tool on every
feature** (ceremony is waste; payoff tracks the feature's hardest question; the
mandatory floor above always runs regardless):

| Hardest question is… | Reach for |
|---|---|
| a pure law / invariant | proptest (always), then Kani if bounded-provable |
| arithmetic / overflow / boundary | clippy (`arithmetic_side_effects`) + Kani |
| concurrency / interleavings / protocol | TLA+ + DST; in-mem race → Loom; real race → TSAN |
| externally-observable behaviour | the BDD scenario (always) + DST for multi-step |
| raw/untrusted input parsing | fuzz |
| spec ↔ code drift | the meta-gates (always on, free) |

## Hard rules (non-negotiable — the gates enforce them)

- **clippy at 4 levels:** `all + pedantic + nursery + cargo` are denied, plus a
  restriction allowlist (`unwrap_used`, `panic`, `indexing_slicing`,
  `arithmetic_side_effects`, `as_conversions`, cognitive-complexity ≤ 15, …).
  Run `dhx clippy` (it uses `--all-features`); a site-level
  `#[allow(..., reason = "…")]` is the ONLY escape and must carry a reason.
- **Determinism bans** (`clippy.toml` `disallowed_methods`): no `SystemTime::now`
  / `Instant::now` / thread RNG in domain code — go through a port.
- **No file over 400 lines** (`check-file-size`, no exemption — split it).
- **Coverage floor** on the verified core (`dhx cov`), proven non-vacuous by
  `dhx mutants`.
- **Every REQ has a BDD+EARS scenario** (mandatory floor; `(verified=…)` markers
  supplement a scenario for non-observable criteria, they never replace it).
- **Conventional-commit prefixes**; hooks always on; **`git --no-verify` is
  forbidden.**
- **`#![forbid(unsafe_code)]`** in shipped crates.

## The dev loop — verify continuously, by wall-clock cost

The tiers are split by **speed**, so the fast verifiers run constantly and only
the genuinely slow ones wait. Run them at this cadence — verification is part of
development, not a pre-release afterthought:

- **`dhx check` — on every file save** (~seconds). fmt + clippy + all the
  meta-gates (traceability, spec-sync, BDD style/coverage, mutation-coverage,
  file-size, docs-counts). Aggregates all failures in one pass.
- **`dhx verify --quick` — after every small change / before each commit**
  (~1-2 min). Adds the unit + property tests, coverage, Kani, **and the spec
  checks: TLA+/TLC model-checking + its anti-vacuity mutation** (the spec is
  verified as early as the code), plus deny/gitleaks/machete and one DST seed.
- **`dhx verify --full` — after a big change / before release.** Adds only the
  expensive thoroughness / discovery instruments: cargo-mutants, TSAN (rebuilds
  std), Loom, fuzz, and the multi-seed DST sweep. Run it when you finish a
  feature, not on every save.

Every tier runs **inside the `dhx:latest` image** — there is no host `dhx`.
Define this shell function once (it mounts cache volumes, so the second run
onward only recompiles what changed — without them every run rebuilds the whole
dependency tree and a warm `verify --quick` of ~35 s becomes a cold ~165 s):

```sh
dhx() { docker run --rm -v "$PWD":/work -w /work \
          -v dhx-cargo-registry:/root/.cargo/registry \
          -v "dhx-target-$(basename "$PWD")":/work/target \
          dhx:latest dhx "$@"; }
```

Then call `dhx check` / `dhx verify --quick` / `dhx verify --full` as local
commands. (`dhx-cargo-registry` is safe to share across projects; the
per-project `dhx-target-*` is not.) Use the `/check` and `/verify` skills.
Commits use conventional-commit prefixes (`feat`/`fix`/`test`/`refactor`/…),
scoped `(REQ-NNN)` for behaviour; the `commit-msg` hook enforces it and
`git log --grep=REQ-NNN` becomes the traceability matrix.

## Harness-first review

Read the harness output before the diff: which proofs ran, which seeds tested,
coverage delta, traceability diff, mutation score. The gates catch what review
misses.

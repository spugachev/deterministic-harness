# seats — project conventions for Claude

This is a **Deterministic Harness** project: a verified Rust service scaffolded
by `dhx`. The verification toolchain is the whole quality story — there is no CI;
the `dhx` gates run locally and are the entire gate.

> **NEVER read or search the `dhx` tool's source (`harness/src/`, the
> deterministic-harness repo).** It is the tool's internals, not your project's
> contract — reading it teaches you nothing you can act on and is forbidden.
> In a normal install it isn't even reachable: your project is standalone and the
> gates run in a container where only your project is mounted. Stay inside THIS
> project directory; do not `cd ..` out of it or `grep` parent directories.
>
> Everything you need to satisfy the gates — the exact `harness.toml` schema, the
> REQ/marker/Kani conventions, the file paths — is in **"Contracts reference"**
> below. Treat the gates as a black box: write code per the contracts, run
> `dhx check`, and fix exactly what the gate output names. When you need a
> template, copy the shipped `domain::example` + its `REQ-001` + `.feature` +
> the `#[cfg(kani)]` proof — never the tool.

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
spec/requirements/    REQ-NNN-*.md   (EARS acceptance criteria, frontmatter)
spec/features/        *.feature      (EARS Gherkin, one+ scenario per REQ)
spec/tla/             *.tla + *.cfg + mutations.toml (generated for an FSM)
spec/adr/             ADR-NNNN-*.md  (architecture decisions)
crates/core/          IO-free verified core: src/domain/, src/ports/, tests/bdd.rs
crates/<outer>/       optional IO crates (axum/db) behind the ports
```

## Contracts reference — write to THESE, don't read the tool

Everything the gates require, with copy-paste forms. The shipped `domain::example`
(REQ-001 + `spec/features/example.feature` + `crates/core/tests/bdd.rs`) is a
working instance of every one of these — copy it.

**`harness.toml` schema** (the only manifest; unknown keys are rejected):

```toml
[meta]
schema_version = 1
[project]
name = "seats"
[coverage]
core = ["core"]              # crate(s) held to the coverage bar (default 90%)
[targets]                    # all optional; omit a key to skip that gate
tsan = "core"                # crate to run under ThreadSanitizer (--lib unit tests)
loom = "core"                # crate with loom:: model tests (skips if it has none)
dst  = { crate = "api", test = "dst" }   # `cargo test -p api --test dst`
fuzz = ["parse_input"]       # fuzz target names under fuzz/fuzz_targets/
[fsm]                        # ONLY if the domain is a state machine
source = "crates/core/src/domain/<file>.rs"
fn_name = "next"             # pure `fn next(State, Event) -> Option<State>`
state_enum = "State"
event_enum = "Event"
generated_stem = "Lifecycle" # → spec/tla/Lifecycle.{tla,cfg}, made by `dhx regen`
```

**REQ frontmatter** (`spec/requirements/REQ-NNN-slug.md`):

```markdown
---
id: REQ-001
title: Holds never oversell the venue
status: active
acceptance:
  - The service shall reject a hold when fewer than N seats are free.
  - Confirming an expired hold shall fail. (verified=proptest)   # optional marker
implements_in:                         # sub-keyed by technique; values are path lists
  gherkin: [spec/features/holds.feature]
  code: [crates/core/src/domain/seats.rs::hold]
  tla: [spec/tla/Lifecycle.tla]        # only if you wrote a spec
---
Prose rationale here.
```
- `implements_in` keys are `gherkin` / `code` / `tla` / `kani` / `proptest` /
  `dst`; a `code` entry may name a `file.rs::symbol`. `check-traceability`
  requires each listed target to exist.
- A `(verified=kani|proptest|dst|tla)` marker at the end of a criterion satisfies
  *token-matching* for a non-HTTP-observable criterion — it does NOT remove the
  need for a tagged BDD scenario (every REQ still needs one), and a marker token
  must be backed by a matching `implements_in.<token>` entry.

**BDD scenario** (`spec/features/*.feature`) — tag each with its REQ id:

```gherkin
Feature: Seat holds
  Scenario: REQ-001 reject when full
    Given a venue with 0 free seats
    When a client requests 1 seat
    Then the service shall reject the hold
```

**Kani proof** (in the core, behind `#[cfg(kani)]`). Kani is a BOUNDED model
checker: it compiles your code + assertions to a SAT/SMT formula (CBMC) and
proves the property holds for **every** value of the symbolic inputs within the
bounds you set. Unlike proptest (samples) it is exhaustive — but only up to the
bounds, and its cost grows with the *size of the formula*.

**The cost model (this is the whole game).** Formula size ≈ the **product** of
every symbolic dimension: bits of symbolic input × loop-unwind depth × heap
cells modelled × branches. A proof is tractable when that product is small and
blows up (CBMC OOM / "running out of memory" / hours) when any one factor is
large or unbounded. So the rules below are about keeping each factor small — not
about banning any particular type.

**Default shape — prefer it; it is tractable by construction:** prove the
*invariant-preserving step* of a pure function on a handful of **scalar**
`kani::any()` inputs. Model aggregate state as a scalar (a count/sum), not as the
container that holds it.

```rust
#[cfg(kani)]
mod proofs {
    use super::*;
    #[kani::proof]
    fn hold_never_oversells() {
        let capacity: u32 = kani::any();
        let held: u32 = kani::any();
        let req: u32 = kani::any();
        kani::assume(held <= capacity);                 // invariant as precondition
        if let Some(new_held) = grant(capacity, held, req) {
            assert!(new_held <= capacity);              // … preserved by the op
        }
    }
}
```

**Universal rules (in priority order):**

1. **Bound every symbolic input.** `kani::assume(x <= SMALL)` (or use a narrow
   type like `u8`). An unbounded `u64`/`usize` driving a length or loop is the
   most common blow-up. Prove the general law on a small bound — the arithmetic
   that holds for `capacity ≤ 8` is the same logic that holds for all `u32`.
2. **Bound every loop.** Add `#[kani::unwind(N)]` with the smallest `N` that
   compiles. No unbounded/`while`-on-symbolic loops in a proof.
3. **Minimise the symbolic state, not the type.** A small fixed-size symbolic
   `Vec`/array (≤ ~3–4 elements, bounded `unwind`) is fine. What blows up is a
   **symbolic-length** collection, or a collection **multiplied by symbolic
   steps** that mutate it (the formula is the product). If you find yourself
   looping N symbolic operations over a `HashMap`/`BTreeMap`, that product is too
   big — collapse it (rule 4).
4. **Collapse state to a scalar when you can.** If the real invariant is
   arithmetic ("sum of holds ≤ capacity"), refactor it into a pure scalar
   function (`fn grant(capacity, held, req) -> Option<u32>`) and prove THAT. The
   container→scalar correspondence (the map really sums to `held`) and the
   multi-step / sequencing behaviour are covered by **proptest + DST**, which are
   built for exactly that — so you lose no coverage, you route it to the right
   tool.
5. **One property per `#[kani::proof]`.** Several small focused harnesses verify
   faster and fail more legibly than one that tries to prove everything.
6. **A failure that says "CBMC failed / out of memory / unwinding assertion /
   foreign function" is a TRACTABILITY problem, not a bug** — shrink bounds
   (rules 1–4). A failure with a concrete counterexample trace IS a real bug —
   fix the code. (`dhx`'s 300 s per-harness backstop turns a runaway into a named
   failure instead of a hang.)

**What Kani is NOT for** (use the routed tool instead): whole-program / async /
IO behaviour (→ DST), multi-step sequences over real collections (→ proptest +
DST), floating-point or unbounded ∀ (→ proptest). Kani's sweet spot is a small,
pure, bounded function with an arithmetic/structural invariant.

**Determinism ports** — domain code calls these, never `SystemTime::now` etc.:
`Clock::now_unix() -> i64`, `Rng::next_u64() -> u64`, `IdGen::next_id() -> u64`.
Tests use the shipped `FixedClock(i64)` / `SeqGen(u64)`. (See `crates/core/src/ports/`.)

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

# Seat-reservation service: harness ON vs OFF

Two independent headless `claude -p` agents built the **same** seat-reservation
service from the **identical** domain spec (`hold → confirm → release`, lazy TTL
expiry, the capacity invariant "confirmed + held ≤ capacity, never overbook",
availability query, safe under concurrent holds). The only variable is the
process wrapper:

- **OFF** — `service-no-harness/` — bare `cargo`, "ship it fast" brief.
- **ON** — `service-with-harness/` — a `dhx init` scaffold, told to follow the
  spec-first workflow until `dhx check` + `dhx verify --quick` are green.

Everything below was **verified independently** (re-run, not taken from the
agents' self-reports).

## At a glance

| | OFF (no harness) | ON (harness) |
|---|---|---|
| Build wall-clock (`claude -p`) | **163 s** (~2.7 min) | **1290 s** (~21.5 min) |
| Rust files / LOC | 1 / **248** | 13 / **1320** |
| Spec artifacts | 0 | **20** (7 REQ + 7 BDD features + TLA+ + 2 ADR + mutations) |
| Architecture | one `main.rs`, state behind one `Mutex` | IO-free `core` behind Clock/Rng/IdGen ports + outer `api` (axum) |
| Tests | **1** smoke | **25** unit/BDD + **proptest** + **2 Kani proofs** + **TLC** + **DST** (64 seeds × 200 steps) |
| Requirements covered by a test | 1 of 6 | 7 of 7 |
| Gate state | n/a | `dhx check` + `dhx verify --quick` **green** |

## The headline finding (it cuts *against* the harness)

**The move-fast OFF arm got the marquee "no overbooking" invariant right.** Its
`Arc<Mutex<Store>>` takes the capacity check and the insert under one lock and
sweeps expired holds at the top of every handler — so the last-seat race is
serialized and the invariant genuinely holds (verified by a 200-way concurrent
hold test on this exact design). The harness's heavy concurrency tooling (Loom /
TSAN / multi-node TLA+) would have found **nothing** on this domain.

This is the honest result: **payoff tracks a feature's bug-surface, not the
ceremony applied to it.** Where OFF's hardest question (an atomic counter) was
easy, the harness was overhead — and it cost ~8× the wall-clock and ~5× the code.

## Where the harness *did* pay off — three concrete, verified gaps

### 1. Expiry is untestable in OFF; deterministic in ON

OFF hardwires `Instant::now()` (4 call sites, **no clock seam**). The whole
expiry requirement ships with **0 tests** — you cannot exercise the TTL path
without a literal 120-second `sleep`. It would only ever break in production.

ON routes time through a `Clock` port (`now_unix() -> i64`), so time is a
parameter: it tests `confirm at now=61 > ttl=60 → rejected`, `expired hold frees
its seats`, etc. — the exact path OFF cannot reach. (The agent even caught a real
off-by-one its own suite flagged: a test asserting a hold was live at
`now == expires_at`, vs. the correct `now < expires_at` boundary.)

### 2. Safe-by-luck vs safe-by-construction arithmetic

OFF's `available()` is raw `capacity - confirmed - held` — clippy's restriction
lints flag **18 sites** (`arithmetic_side_effects` + `unwrap`). It doesn't
underflow *today* only because the invariant happens to hold; one careless
refactor wraps it and overbooks, with no test to catch it.

ON uses checked/saturating arithmetic and the no-overbooking invariant is
**proven, not assumed**:
- **proptest** — random op-sequences assert `confirmed + held ≤ capacity` throughout.
- **Kani** — `grant_never_oversells` / `grant_rejects_when_no_room` prove it
  exhaustively over **every** `(capacity, occupied, req)` in 0.6 s.
- **TLC** — model-checks the hold-lifecycle FSM, with 2 anti-vacuity mutations
  proving the invariants aren't trivially true.

### 3. Spec ↔ code traceability

OFF has no requirements doc, so "did we build all of it?" is unanswerable — and
indeed only 1 of 6 behaviours is tested. ON has 7 EARS `REQ-NNN.md`, each with a
mandatory tagged BDD scenario (`check-bdd-coverage` enforces it), the FSM
projected to a checked TLA+ spec, and a green traceability matrix.

## Bug-class scorecard for *this* domain

| Bug class | OFF exposure | Harness tool | Paid off here? |
|---|---|---|---|
| Overbooking under concurrency | none (global mutex correct) | Loom/TSAN/TLA+ | **No** — no bug to find |
| Hold-expiry / time logic | **untested, prod-only** | Clock port + unit tests | **Yes** |
| Arithmetic underflow on refactor | latent, no test | clippy + Kani + proptest | **Yes** (future-proofing) |
| Requirement left unbuilt/untested | 5 of 6 untested | REQ + BDD coverage gate | **Yes** |

## On the cost — and what we learned building this

ON took ~21 min vs OFF's ~2.7. But this experiment also drove three fixes to the
harness itself, because building a *real* service is what exposes a tool's rough
edges (all now fixed in `dhx`):

- **Miri removed** — it only finds UB, which `#![forbid(unsafe_code)]` already
  precludes; it was 10 min of interpreting the async test for zero coverage.
- **TSAN/Loom gates fixed** — they ran the cucumber BDD harness (incompatible)
  and forced `--cfg loom` onto tokio; now scoped to lib unit tests / skip-by-shape.
- **CLAUDE.md given a "Contracts reference" + calibrated Kani rules** — an
  earlier ON agent spent ~30 min reading the tool's own source to reverse-engineer
  the gate contracts, and wrote a Kani proof that OOM'd CBMC (symbolic collection
  × symbolic steps). With the contracts documented and the Kani cost-model rules
  in place, this run wrote tractable scalar proofs and never read the tool source.

The warm gate loop is cheap (`dhx check` <1 s, `verify --quick` 22 s with cache
volumes); the 21 min was authoring + the agent learning an unfamiliar workflow,
not the gates.

## Verdict

The harness is **not** a universal win, and this experiment is honest about it:
on the one thing OFF was most likely to get wrong — the concurrency invariant — a
careful build got it right, and the harness's concurrency stack was dead weight.

Its value showed up exactly where theory predicts: the determinism seam made a
time-dependent requirement **testable at all** (OFF: 0 expiry tests), proofs
replaced "safe by luck" arithmetic with "safe by construction," and the
spec/coverage gates turned "did we build the whole thing?" from a hope into a
checked fact (7/7 vs 1/6). For a throwaway prototype, OFF is the right call. For a
service where a missed expiry or a refactor-induced overbook is a real-money
incident, the harness buys verification you cannot get from "it compiles and the
one test passes."

---

### Reproduction

```sh
# OFF
cd service-no-harness && cargo test                     # 1 test
# ON (gates run in the dhx image via the cached alias)
dhx() { docker run --rm -v "$PWD":/work -w /work \
          -v dhx-cargo-registry:/root/.cargo/registry \
          -v "dhx-target-$(basename "$PWD")":/work/target dhx:latest dhx "$@"; }
cd service-with-harness
cargo test --workspace      # 25 tests
dhx check                   # 11 meta-gates green (<1 s warm)
dhx verify --quick          # + proptest, Kani, TLC, DST (22 s warm)
```

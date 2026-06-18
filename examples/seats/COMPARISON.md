# Seat-reservation service: harness ON vs OFF

Two independent headless `claude -p` agents built the **same** seat-reservation
service from the **identical** domain spec ([`/tmp/seats-domain-goal.md`], copied
below). The only variable is the process wrapper:

- **OFF** — `service-no-harness/` — bare `cargo`, "ship it fast" brief.
- **ON** — `service-with-harness/` — the dhx-scaffolded harness, told to follow
  the spec-first workflow until `dhx check` is green.

Both compile and both pass their own tests. Everything below was **verified
independently** (re-run, not taken from the agents' self-reports).

## At a glance

| | OFF (no harness) | ON (harness) |
|---|---|---|
| Rust files | 1 (`main.rs`) | 14 across `core` + `api` |
| Lines of Rust | 276 | 1480 |
| Spec artifacts | 0 | 12 (6 REQ + BDD feature + TLA+ + ADR) |
| Architecture | one file, state behind one `Mutex` | IO-free `core` behind Clock/Rng/IdGen ports + outer `api` (axum) |
| Tests | **1** happy-path smoke | **28** (`#[test]`) + 3 proptest + 2 Kani proofs + 10 BDD scenarios + 1 DST |
| Requirements covered by a test | 1 of 6 | 6 of 6 |
| Build time | seconds | ~63 min (agent iterated the full docker gate loop to green) |

## The headline finding (it cuts *against* the harness)

**The move-fast OFF arm got the marquee "no overbooking" invariant right.** I
threw 200 concurrent hold requests at capacity 100; `confirmed + held` never
exceeded 100. A single global `Mutex` plus "purge expired before every
operation" is genuinely correct for a single-node counter — so the harness's
heavy concurrency tooling (Loom / TSAN / multi-node TLA+) would have found
**nothing** on this domain. Effort spent there would have been pure overhead.

This is the honest, important result: **payoff tracks a feature's bug-surface,
not the ceremony applied to it.** Where OFF's hardest question (atomic counter)
was easy, the harness added cost, not safety.

## Where the harness *did* pay off — three concrete, verified gaps

### 1. Expiry is untestable in OFF; a deterministic test in ON

OFF hardwires `Instant::now()` with a 120 s `const` TTL and no seam. The whole
**expiry requirement ships with zero tests** — you cannot exercise it without a
literal 120-second `sleep`. It would only ever break in production.

ON routes time through a `Clock` port (`now_unix() -> i64`), so time is a
parameter. It ships two tests that exercise the exact path OFF cannot:

```rust
fn confirm_expired_hold_is_rejected() {
    let mut r = Reservation::new(10, /*ttl*/ 60);
    let h = r.hold(4, &FixedClock(0), &mut ids).unwrap();
    assert_eq!(r.confirm(h.id, &FixedClock(61)), Err(ConfirmError::NotConfirmable));
    assert_eq!(r.available(61), 10);            // seats reclaimed
}
fn expired_hold_does_not_block_new_hold() { /* hold 10 @ t=0, re-hold 10 @ t=61 */ }
```

Same behaviour in the spec, one arm can prove it in microseconds and the other
can't test it at all. That is the determinism seam earning its keep.

### 2. Safe-by-luck vs safe-by-construction arithmetic

OFF: `CAPACITY - self.confirmed - self.held()` — raw subtraction. clippy's
`arithmetic_side_effects` (a harness lint) flags it at three sites. It does not
underflow *today* — only because the invariant happens to hold. One careless
refactor (a confirm path that forgets to purge) silently wraps to a huge
"available" and overbooks, with **no test to catch it**.

ON: `total.saturating_sub(confirmed).saturating_sub(held(now))` — cannot
underflow by construction, and the no-overbooking property is **proven**, not
assumed:

- **proptest** `capacity_invariant_never_overbooks` — 40-op random sequences
  over random capacity/TTL, asserting `confirmed + held <= capacity` throughout.
- **Kani** `no_overbooking_under_operations` / `available_never_underflows` —
  exhaustive over all bounded inputs, not sampled.

### 3. Spec ↔ code traceability, and a checked FSM

OFF has no requirements document, so "did we build all of it?" is unanswerable —
and indeed only 1 of 6 behaviours is tested. ON has 6 EARS `REQ-NNN.md`, each
with a mandatory BDD+EARS Gherkin scenario (`check-bdd-coverage` enforces it),
the hold lifecycle modelled as a pure `fn next` FSM, and `dhx regen` projecting
it to a TLA+ spec that TLC model-checks — with anti-vacuity mutations proving the
invariants aren't trivially true.

## Bug-class scorecard for *this* domain

| Bug class | OFF exposure | Harness tool | Paid off here? |
|---|---|---|---|
| Overbooking under concurrency | none (global mutex correct) | Loom/TSAN/TLA+ | **No** — no bug to find |
| Hold-expiry / time logic | **untested, prod-only** | Clock port + unit tests | **Yes** |
| Arithmetic underflow on refactor | latent, no test | clippy + Kani + proptest | **Yes** (future-proofing) |
| Requirement left unbuilt/untested | 5 of 6 untested | REQ + BDD coverage gate | **Yes** |
| Panic on poisoned lock | `lock().unwrap()` ×4 | clippy `unwrap_used` | Minor |

## Verdict

The harness is **not** a universal win and this experiment is honest about that:
on the one part OFF was most likely to get wrong — the concurrency invariant — a
careful developer (or model) got it right, and the harness's expensive
concurrency stack would have been dead weight.

Its value showed up exactly where theory predicts: **the determinism seam made a
time-dependent requirement testable at all**, **proofs replaced "safe by luck"
with "safe by construction"**, and **the spec/coverage gates turned "did we build
the whole thing?" from a hope into a checked fact** (6/6 vs 1/6).

The cost is equally real and equally honest: ~5× the code, a spec to maintain,
and a ~60-minute green-the-gates loop dominated by in-container compilation. For
a throwaway prototype, OFF is the right call. For a service where a missed
expiry or a future refactor that overbooks is a real-money incident, the harness
buys verification you cannot get from "it compiles and the one test passes."

---

### Reproduction

```sh
# OFF
cd service-no-harness && cargo test                     # 1 test
# ON  (gates run in the dhx image)
cd service-with-harness && cargo test --workspace       # 28 tests
docker run --rm -v "$PWD":/work -w /work dhx:latest dhx check         # 11 gates green
docker run --rm -v "$PWD":/work -w /work dhx:latest dhx verify --quick # + TLC, Kani, proptest, DST
```

### The shared domain goal

Hold N seats (TTL expiry) → confirm before expiry → release (idempotent) →
lazy expiry frees seats → **capacity invariant: confirmed + held ≤ capacity, no
overbooking ever** → availability query; safe under concurrent holds.

# Seat-reservation service: harness ON vs OFF

Two independent headless `claude -p` agents built the **same** seat-reservation
service from the **identical** domain spec (`hold → confirm → release`, lazy TTL
expiry, the capacity invariant "confirmed + held ≤ capacity, never overbook",
availability query, safe under concurrent holds). Only the process wrapper differs:

- **OFF** — `service-no-harness/` — bare `cargo`, "ship it fast" brief.
- **ON** — `service-with-harness/` — a `dhx init` scaffold, taken to green
  `dhx check` + `dhx verify --quick`.

Both built on the **same (fixed) harness** as the sibling `../ledger` A/B, so the
two are directly comparable. Everything below was **verified independently**
(re-run / re-probed, not taken from the agents' self-reports).

## At a glance

| | OFF (no harness) | ON (harness) |
|---|---|---|
| Build wall-clock (`claude -p`) | **145 s** (~2.4 min) | **3053 s** (~51 min) |
| Rust files / LOC | 2 / **~250** | 13 / **1391** |
| Spec artifacts | 0 | 6 REQ + BDD (12 scenarios) + TLA+ + 2 ADR + mutations |
| Tests | **1** smoke | **25** + proptest + **2 Kani** + **TLC** + **DST** |
| Requirements covered by a test | 1 of 6 | 6 of 6 |
| Gate state | n/a | `dhx check` + `dhx verify --quick` **green** |

## The headline finding — OFF has NO latent bug; the harness was mostly overhead

Unlike the `ledger` A/B (where OFF shipped a real, review-passing conservation
bug), **the move-fast seats build is correct.** I probed it adversarially and
found nothing to catch:

- **No overbooking.** 50 concurrent-style holds against capacity 5 → at most 5
  granted; `capacity - committed` never underflows. The global `Mutex` + the
  `seats > available` guard serialize the last-seat race correctly. Loom / TSAN /
  TLA+ would find nothing.
- **Expiry is actually testable here.** This OFF agent threaded `now: Instant` as
  a *parameter* of `hold`/`confirm`/`release`/`available` — a real time seam (even
  without a `Clock` trait). My probe drove a hold to expiry at `t0 + 61s` with no
  real sleep: seats freed, confirm rejected. **The seam exists.**

So seats sits at the opposite end of the spectrum from ledger: an atomic-counter
domain whose hardest question (the capacity race) a careful build gets right.

## So where did the harness add anything?

Its value on seats is **narrower and preventive**, not bug-catching:

1. **It would force the missing tests.** OFF *can* test expiry but **didn't** —
   1 smoke test, 1 of 6 behaviours covered, the TTL path among the 5 untested.
   ON has 6 REQ each with a mandatory tagged BDD scenario, a proptest capacity
   law (`confirmed + live_held ≤ capacity` after every op in a sequence), and
   deterministic expiry tests via the `Clock` port (`now < expires_at`, Unix
   seconds). "It works" vs "it's checked, and stays checked under refactor."
2. **It proves the invariant instead of trusting the mutex.** Two **tractable
   scalar Kani proofs** (`grant_step_never_oversells`, `grant_step_decision_is_exact`)
   verify the capacity arithmetic over **every** `u32` `(capacity, confirmed, held)`
   in 0.6 s. OFF's correctness rests on the lock design + one happy-path run.
3. **Spec ↔ code traceability.** OFF has no requirements doc, so "did we build all
   6 behaviours?" is unanswerable (and indeed 5 are untested). ON's FSM is
   projected to a model-checked TLA+ spec (3 transitions, 2 anti-vacuity
   mutations) and every REQ is linked.

None of these *caught a bug* — there wasn't one. They convert "correct today,
by a careful author" into "proven, and protected against the next edit."

## Cost — and the honest read

| | seats | (cf. ledger) |
|---|---|---|
| OFF latent bug found | **none** | a real conservation overflow |
| Harness role | force tests + prove a correct invariant | **catch a review-passing bug** |
| Build cost | 51 min ON vs 2.4 min OFF | 17 min ON vs 3 min OFF |

ON seats also cost the most wall-clock of any run (~51 min, 28 gate iterations) —
the agent churned on BDD-coverage and a doc-comment quirk in the FSM extractor
before reaching green. The warm gate loop itself is cheap (`dhx check` 3 s,
`verify --quick` 13–28 s); the 51 min is authoring + iteration, not the gates.

## Verdict — read this next to ../ledger/COMPARISON.md

These two A/Bs are the honest pair:

- **seats** — a simple, mutex-guarded counter domain. The OFF build is *correct*;
  the harness pays for proof + coverage + traceability it didn't strictly need to
  ship working software. **Overhead-dominant.**
- **ledger** — conservation over unbounded integer arithmetic. The OFF build
  shipped a silent money-destroying overflow behind a plausible-but-false safety
  comment; the harness eliminated it by construction and *proved* it away.
  **Payoff-dominant.**

The single rule both confirm: **payoff tracks the feature's bug-surface, not the
ceremony applied to it.** A capacity counter has a small bug-surface and the
harness is mostly insurance; a money-conservation invariant has a large one and
the harness earns its cost outright. The craft is knowing which domain you're in
— and routing the expensive tools accordingly.

---

### Reproduction

```sh
# OFF
cd service-no-harness && cargo test            # 1 test (expiry/capacity untested but correct)
# ON (gates in the dhx image via the cached alias)
dhx() { docker run --rm -v "$PWD":/work -w /work \
          -v dhx-cargo-registry:/root/.cargo/registry \
          -v "dhx-target-$(basename "$PWD")":/work/target dhx:latest dhx "$@"; }
cd service-with-harness
cargo test --workspace      # 25 tests
dhx check                   # 11 meta-gates green (3 s warm)
dhx verify --quick          # + proptest, Kani (all u32 triples), TLC, DST (13-28 s warm)
```

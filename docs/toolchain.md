# Toolchain — every tool, what it does, and what it's worth

The single reference for the verification toolchain: **what each tool does**, a
**runnable command**, the **class of bug it closes**, and a **usefulness
rating**. Ratings are practical bug-catching ROI, graded from evidence — the
A/B studies and planted-defect probes run on the prototype this harness was
extracted from — not raw capability. ★★★★★ = repeatedly pays off; ★★ = kept for
a real but narrow/insurance reason, honestly flagged.

Two framing rules:

- **One tool per bug class.** Every tool catches a class no other tool catches;
  a duplicate would add wall-clock and "which is authoritative?" debates for no
  safety gain.
- **Route, don't spray.** A cheap always-on floor (clippy + proptest + the
  meta-gates) plus heavy instruments aimed where the feature's hardest question
  lands. See the rubric in [workflow.md](workflow.md).

Tier column = the cheapest tier the tool runs in (`check` / `quick` / `full`).

---

## Part 1 — the tools, one by one

### clippy (4 levels + restriction allowlist) — ★★★★★ · `check`
**What.** Rust's linter configured far past default: `all + pedantic + nursery +
cargo` denied, plus a hand-picked restriction allowlist
(`arithmetic_side_effects`, `as_conversions`, `cast_lossless`,
`indexing_slicing`, `unwrap_used`, `panic`, `disallowed_methods`,
cognitive-complexity ≤ 15, …). The always-on floor.
**Run.** `dhx clippy` (uses `--all-features`; a bare `cargo clippy` reports false
positives outside that).
**Example.** An unguarded `total += weight` is refused by
`arithmetic_side_effects` before any test runs; `100 - wa` underflow and a
panicking `OffsetDateTime - OffsetDateTime` are caught the same way.
**Bug class.** Antipatterns, unchecked arithmetic, lossy casts, reachable panics
(`unwrap`/`[]`/`panic!`), excess complexity, direct non-determinism.
**Why ★★★★★.** Best ROI in the stack — ~zero marginal cost, caught 3 of 6 bugs
in the first A/B study before any test ran.

### proptest — ★★★★★ · `quick`
**What.** Property-based testing: hundreds of random inputs assert a *law*
(idempotence, monotonicity, round-trip, bounds), shrinking any failure.
**Run.** part of `dhx test`; inline `proptest! { }` blocks.
**Example.** `next_is_total` asserts the FSM transition never panics for any
`(state, event)`; bound laws catch a wrapping accumulator.
**Bug class.** Violations of a pure law across a wide input space — what a
happy-path example test misses.
**Why ★★★★★.** The workhorse: broad reach over any pure function, cheap, found
real wire-format and trim bugs in the prototype.

### Kani (CBMC bounded model checker) — ★★★☆☆ · `quick`
**What.** Symbolic *bounded* proof: replaces inputs with `kani::any()`, explores
all values in bounds, proves totality / panic-freedom / arithmetic invariants.
**Run.** `dhx kani` (full proof) · `dhx check-kani-codegen` (cheap compile-only
rot check, in `quick`).
**Bug class.** Arithmetic/structural invariants on pure, bounded, total
functions — proven over *every* in-range input, not sampled.
**Why ★★★☆☆.** High value in its domain (it refuted an over-strong proof in the
prototype) but carries operational debt — harnesses can be intractable, so a
`--harness-timeout` backstop fails a runaway by name instead of hanging.

### Verus (Z3 deductive verifier) — ★★☆☆☆ · `full`
**What.** Deductive (SMT) proof of pure functions with quantified/nonlinear
postconditions — Kani's ∀-over-all-integers twin. Opt-in (`[verus]`).
**Run.** `dhx verus`.
**Bug class.** True ∀ / nonlinear postconditions bounded checking can't fully
close (e.g. a ceil-division that is sufficient *and* tight over all integers).
**Why ★★☆☆☆.** Weakest on *unique* bug-finding — found no bug Kani wouldn't, and
costs a standalone duplicate kept in sync by `check-spec-sync`. Kept as the
deductive contrast to Kani; the first tool to drop on a tight budget.

### TLA+ / TLC (specification model checker) — ★★★☆☆ · `full`
**What.** Model-checks a *specification* of the state machine / concurrency
protocol, exploring all reachable states for invariant violations. The FSM spec
is generated from `state.rs::next` by `dhx regen`; concurrent specs are
hand-written.
**Run.** `dhx tlc` (model-check) · `dhx tlc --mutate` (anti-vacuity).
**Bug class.** Spec-level concurrency/protocol errors: bad interleavings,
forbidden states, a vacuous invariant.
**Why ★★★☆☆.** Applies only when a feature is concurrent — but then it catches
what nothing else can. The `regen` link means the model can't drift from the code.

### DST — deterministic simulation testing (turmoil) — ★★★★☆ · `quick`
**What.** Runs the **real app** in a simulated network with a mocked clock and a
seeded shadow oracle; drives multi-step sequences, injects faults, and replays
any failure from its seed.
**Run.** `dhx dst --seed <n> --iterations <k>` (`--seed random` = discovery).
**Bug class.** Full-stack, multi-step, multi-row, over-the-network sequence bugs
+ fault-injection behaviour.
**Why ★★★★☆.** Indispensable for multi-step/retry/network behaviour; caught a
version-arithmetic bug across a create→patch→delete sequence in the prototype.
Not ★★★★★ only because scenarios are expensive to author.

### Loom — exhaustive in-memory concurrency checker — ★★★★☆ · `full`
**What.** Enumerates **every** thread interleaving of a shared-memory pattern.
**Run.** `dhx loom`.
**Bug class.** In-memory data races / lost updates — about *schedules*, not
inputs, so invisible to proptest/Kani/single-thread tests.
**Why ★★★★☆.** Narrow but unique; found a lost-update no other tool could, in
milliseconds.

### ThreadSanitizer (TSAN) — ★★★☆☆ · `full`
**What.** Runtime data-race detector on the real (tokio/threaded) stack —
recompiles std with `-Zsanitizer=thread`. The one axis Loom can't model.
**Run.** `dhx tsan` (pinned nightly, `-Zbuild-std`).
**Bug class.** Real-thread data races (UB), whether or not a given schedule lost
an update.
**Why ★★★☆☆.** Real guarantee on the production stack; insurance until shared
mutable state appears, then it fires (proven on a planted race).

### Miri — undefined-behaviour interpreter — ★★☆☆☆ · `full`
**What.** Interprets under an abstract machine that detects UB: invalid enum
discriminants, OOB, use-after-free, bad `transmute`, races.
**Run.** `dhx miri` (pinned nightly).
**Bug class.** Memory/UB errors invisible to the compiler and ordinary tests.
**Why ★★☆☆☆.** Near-zero value while `#![forbid(unsafe_code)]` holds — insurance
for *future* `unsafe`; decisively valuable the instant raw memory appears
(proven on a planted `transmute` UB).

### cargo-fuzz (libFuzzer) — ★★★☆☆ · `full`
**What.** Coverage-guided fuzzing: mutated random bytes hunt for panics; a crash
persists as a replayable artifact.
**Run.** `dhx fuzz <target>`.
**Bug class.** Panics/crashes on arbitrary raw input — parsers, decoders, any
`&[u8]`/`&str` byte-fiddling.
**Why ★★★☆☆.** High value on untrusted-input boundaries (crashed a hand-indexed
parser in <1 s in the prototype), low on already-typed domain logic.

### cargo-mutants — mutation testing — ★★★★☆ · `full`
**What.** Mutates the *product* code (`>`→`>=`, delete a stmt, …) and reruns the
tests; a surviving mutant is a test that doesn't actually check that behaviour.
**Run.** `dhx mutants`.
**Bug class.** Weak/ineffective tests — a *meta* defect, not a product bug.
**Why ★★★★☆.** Unique: nothing else finds an inverted-logic-still-passes test.
Found a real test gap proptest had missed. Not ★★★★★ only because it's expensive
(a rebuild per mutant).

### The intent-drift meta-gates — ★★★★☆ as a group · `check`
Cheap deterministic gates that keep *other tools and the docs honest*. They
catch the harness lying to itself.

| Gate | Enforces |
|---|---|
| `check-traceability` | every REQ's `implements_in` path/symbol exists; the REQ↔ADR↔TLA↔code graph is intact |
| `check-spec-sync` | the runtime FSM, the generated spec, and any Verus duplicate agree transition-by-transition |
| `check-bdd-style` | Gherkin/EARS grammar; every Then names the system |
| `check-bdd-coverage` | every REQ acceptance criterion is covered by a scenario or a `verified=` marker |
| `check-verified-markers` | every `(verified=kani/verus/tla)` marker has a real backing link |
| `check-mutation-coverage` | every `.cfg` invariant has a mutation **or** a justified exemption — no vacuous invariant |
| `check-file-size` | no `.rs` over 400 lines (no exemption — split it) |
| `check-docs-counts` | the README `<!-- dhx:counts -->` region matches reality |

**Why ★★★★☆.** They catch no product bug — they prevent the silently-toothless
gate. Cheap, high-signal, and the reason the rest of the harness can be trusted.

### Supply chain & hygiene
- **cargo-deny** — ★★★★☆ · `quick`. RUSTSEC advisories + license allowlist +
  banned crates + sources. `dhx deny`. Catches a class compilation can't.
- **cargo-machete** — ★★★☆☆ · `quick`. Unused dependencies. `dhx machete`. Dead
  weight = attack surface.
- **cargo-llvm-cov** — ★★★★☆ · `quick`. Region/branch coverage on the verified
  core, ignore-set derived from `cargo metadata`. `dhx cov`. The quantitative
  adequacy floor (proven non-vacuous by mutants).
- **gitleaks** — ★★★★☆ · `quick`. Committed-secret scanner — **must** inherit
  default rules (`useDefault = true`) or it is silently toothless. `dhx gitleaks`.
- **cargo-outdated / cargo-geiger** — ★★☆☆☆ · `full`. Dependency staleness /
  unsafe-surface trend. Soft signals, never block (`geiger` is advisory,
  non-gating — its result is not asserted).

### Structural guarantee
- **`#![forbid(unsafe_code)]`** per shipped crate — a *compile error* if
  violated, stronger than any geiger gate. The unsafe experiment arms (if any)
  are quarantined in a separate crate.

---

## Part 2 — bug class → owning tool (reverse lookup)

Start from the kind of bug you're worried about:

| Bug class | Owning tool |
|---|---|
| antipattern / complexity > 15 | clippy |
| unchecked arithmetic / overflow / lossy cast | clippy (`arithmetic_side_effects`, `cast_*`) |
| reachable `unwrap`/`[]`/`panic!` | clippy (restriction lints) |
| direct `SystemTime::now` / thread RNG in domain code | clippy `disallowed_methods` → a port |
| violation of a pure law (sampled) | proptest |
| arithmetic/structural invariant (bounded ∀) | Kani |
| unbounded ∀ / nonlinear postcondition | Verus |
| spec-level concurrency / protocol error | TLA+ TLC |
| vacuous (toothless) invariant | `tlc --mutate` + `check-mutation-coverage` |
| full-stack multi-step / network sequence | DST |
| in-memory race / lost update | Loom |
| real-thread data race | TSAN |
| memory UB (transmute/OOB/UAF) | Miri |
| raw-input panic | cargo-fuzz |
| weak / ineffective tests | cargo-mutants |
| untested regions | cargo-llvm-cov |
| HTTP-observable behaviour | cucumber (BDD) + DST |
| vulnerable / bad-license / banned dep | cargo-deny |
| unused dependency | cargo-machete |
| leaked secret | gitleaks |
| requirements / spec / doc drift | the meta-gates |
| unsafe in shipped code | `#![forbid(unsafe_code)]` |

## Part 3 — the verification pyramid

Slowest/most-understanding at the top, most-diagnostic at the bottom. Run the
layer that answers your question; the everyday floor is clippy + proptest + the
meta-gates.

1. **TLA+** — understanding of a concurrent protocol.
2. **DST** — the primary integration test (seeded, replayable).
3. **Kani / Verus** — bounded and deductive proofs.
4. **Telemetry / benchmarks** — empirical ground truth.

The decisive finding across the A/B studies: **payoff tracks a feature's
bug-surface, not effort** — decisive on arithmetic / boundary / date /
concurrency / UB / untrusted-input; dead weight on flat CRUD. That is why the
toolchain rewards *routing* over running everything.

# Deterministic Harness (`dhx`)

> An opinionated scaffolder that creates a new Rust service already wired into a
> comprehensive verification toolchain, and one CLI that runs every gate
> locally. There is no CI — the gates *are* the gate.

---

## How

Clone, install the CLI, scaffold a project anywhere, verify it:

```sh
# 1. install the CLI (one time)
git clone https://github.com/spugachev/deterministic-harness
cd deterministic-harness && cargo install --path dhx     # `dhx` on PATH

# 2. scaffold a new service at any path
dhx init ~/code/payments-svc
cd ~/code/payments-svc

# 3. it already IS a verified service
dhx check            # cheap gates, every edit (~seconds)
dhx verify --quick   # + tests, coverage, Kani, DST (pre-push)
dhx verify --full    # everything, in the pinned Docker image (pre-release)
```

`dhx init` writes a workspace with an IO-free core, the Clock/Rng/IdGen ports,
`spec/` (requirements, ADRs, TLA+), `.harness/` (version pins, tool configs, git
hooks), a `clippy.toml` of determinism bans, a `Dockerfile` built from the pins,
and `CLAUDE.md` + `.claude/` (skills `/check` `/verify`, a post-edit hook) so an
agent is wired in from the first commit. The scaffolded project has **no path
dependency** back on this repo.

The three tiers:

| Tier | Command | When | Cost |
|---|---|---|---|
| Preflight | `dhx check` | after every edit | seconds — 11 cheap gates, all failures aggregated |
| Quick | `dhx verify --quick` | before push | + test, coverage, Kani, DST |
| Full | `dhx verify --full` | before release | + Verus, TLA+, Miri, TSAN, Loom, fuzz, mutants — **in Docker** |

`dhx config explain <gate>` shows any gate's resolved value and where it came
from.

---

## Why — the approach

### Successful, but wrong

An agent is told to fix a panic in a handler. It reads the file, adds the
missing check, runs the tests. They pass. It commits. Three days later a
different code path panics in production: the fix shadowed a legitimate error
case. No test exercised that path; the diff was three lines; review caught
nothing. By every measure available to the loop it ran in, the change was a
success — and it was wrong.

This is the problem. As code generation gets cheaper — by an LLM or a hurried
human — the bottleneck moves from *writing* code to *verifying* it. "It
compiles and the tests pass" is not "it is correct." A bigger model does not
close that gap. A tighter, deterministic harness does.

### Oracle trust, not compiler trust

We trust a compiler because the input language has precise semantics and the
transformation to machine code is well-defined. An agent has neither: it ingests
unrestricted natural language and emits code with no contract that the output
matches the intent. So the trust boundary has to move.

- **Compiler trust** — the output is correct because the input is *formal*.
- **Oracle trust** — the output is *checked, after the edit*, by external
  verifiers (type-checker, linter, tests, property checks, model checkers,
  proofs), and the author iterates against their verdict.

The oracle is the trust boundary, not the author. The consequence drives every
decision here: **verifiability bounds what can be created.** Wherever a property
can be checked automatically, more of the work can be delegated; wherever it
cannot, it cannot. The agent generates, the harness verifies, telemetry
validates.

### Determinism is the prerequisite, not the goal

A verifier is only an oracle if it gives the **same verdict for the same input,
every time.** A flaky gate teaches you to ignore it. So the harness makes
determinism a precondition:

- Every gate is deterministic *as a verdict*. Discovery gates (fuzz, proptest,
  random-seed simulation) use entropy to *find* bugs, but a found failure is
  persisted and replays exactly.
- All ambient non-determinism in the domain — wall-clock, randomness, id
  generation — flows through **ports** (`Clock`/`Rng`/`IdGen`). A seed fully
  determines a run, so a failure is reproducible. `clippy.toml` bans the direct
  calls so the discipline cannot erode.
- Tool versions are **pinned**, and `verify --full` runs inside a Docker image
  built *from those pins*, so two machines reach the same verdict.

### Gates with teeth — and the one failure mode that matters

A gate that cannot fail is decoration. The worst outcome in a verification
project is the **silently toothless gate**: one that reports green while
checking nothing — a secret scanner with no rules, a model-checker invariant
that constrains nothing, a coverage gate scoped at the wrong crate. It is worse
than no gate, because it manufactures confidence. The harness is built against
exactly this:

- **Presence ⇒ mandatory.** If a project *looks* like it has something to verify
  (an FSM source on disk, a `.tla` that declares invariants, a `verified=` claim
  in a requirement) but it is not configured, `dhx` **fails loudly** — it never
  silently skips. "Declared out of scope" is auditable; "happened to be absent"
  is not.
- **Anti-vacuity is itself gated.** Every TLA+ invariant must have a
  known-violating mutation the model checker is *required* to catch, so an
  invariant cannot ship vacuous.
- **Test adequacy is gated.** Coverage sets a floor; mutation testing proves the
  tests actually *kill* logic mutations — a test that still passes with the
  logic inverted is a weak test and fails.
- **The verifier verifies itself.** `dhx` runs the universal subset of its own
  gates on its own source (fmt, clippy, tests, ≤400-line files). An unverified
  verifier would be the ultimate toothless gate.

### Comprehensive, but routed — the verification pyramid

The toolchain is *comprehensive* — there is a tool for every class of bug — but
you do **not** run every tool on every feature. Ceremony is waste. The skill the
pipeline encodes is **routing**: spend the heavy instruments only where the
feature's hardest question lands. The layers, slowest/most-understanding at the
top, most-diagnostic at the bottom:

1. **TLA+** — understanding of a concurrent protocol (generated from the code).
2. **DST** — the primary integration test: the real app over a simulated
   network with a mocked clock, seeded and replayable.
3. **Kani / Verus** — bounded and deductive proofs of pure functions.
4. **Telemetry / benchmarks** — empirical ground truth.

Underneath all of it, on every edit at near-zero cost, runs the floor:
clippy + proptest + the intent-drift meta-gates. The decisive empirical finding
behind this design: **payoff tracks a feature's bug-surface, not effort.** The
harness is decisive on arithmetic, boundaries, dates, concurrency, UB, and
untrusted input — and dead weight on flat CRUD.

### The precondition: the shape that gives the gates teeth

These gates are not generic. They verify a *specific architecture*, and that
architecture is what makes them meaningful — which is why `dhx` is a scaffolder,
not a linter you point at any repo:

- **An IO-free verified core.** No DB, HTTP, or async runtime in `crates/core`.
  A pure function has no hidden inputs, so Kani/Verus can prove it total and
  proptest can assert laws over it.
- **All non-determinism behind ports.** This is the seam that makes DST/Loom/
  TSAN deterministic and replayable.
- **A workspace.** Coverage runs the whole suite but reports only on the core,
  so the core's bar counts every test that exercises it — including integration
  tests in other crates.

Persistence and HTTP (axum, sqlx, anything) are *optional outer crates* behind
the ports — never a harness requirement. The harness needs the architecture,
not any library.

This is the right shape for the current era of agentic development, stated
honestly as such — not a claim for all time. The harness is meant to evolve.

---

## The tools — what each is, why it's here, and how it helps

One tool per bug class: each catches a class **no other tool catches**, so a
duplicate would add wall-clock and "which is authoritative?" debates for no
safety gain. Ratings are practical bug-catching ROI (graded from A/B studies +
planted-defect probes on the prototype this harness came from), not raw
capability. "Tier" is the cheapest tier the tool runs in.

| Tool | Tier | Catches (bug class) | How it helps | ★ |
|---|---|---|---|---|
| **clippy** (4 levels + restriction) | check | antipatterns, unchecked arithmetic, lossy casts, reachable panics, complexity, direct non-determinism | the always-on floor; refuses whole bug classes before a test runs | ★★★★★ |
| **proptest** | quick | violation of a pure law (idempotence, monotonicity, round-trip, bounds) | finds the boundary the happy-path test forgot, then shrinks it | ★★★★★ |
| **meta-gates** (traceability, spec-sync, bdd-coverage, mutation-coverage, file-size, docs-counts) | check | spec ↔ code ↔ docs drift; vacuous invariants; the *toothless gate itself* | keep every other gate and the docs honest, ~free | ★★★★☆ |
| **DST** (turmoil) | quick | full-stack multi-step / network / fault-injection sequences | drives the real app over a simulated network; replays any failure from a seed | ★★★★☆ |
| **cargo-mutants** | full | weak tests (logic inverted, tests still green) | proves the test suite has teeth, not just coverage | ★★★★☆ |
| **Loom** | full | in-memory data races / lost updates (about *schedules*, not inputs) | exhausts every interleaving of a shared-memory pattern | ★★★★☆ |
| **cargo-deny** | quick | vulnerable / banned / bad-license dependencies | a supply-chain class compilation cannot see | ★★★★☆ |
| **cargo-llvm-cov** | quick | untested regions of the verified core | the quantitative adequacy floor (proven non-vacuous by mutants) | ★★★★☆ |
| **gitleaks** | quick | committed secrets | catches a key before it leaves your machine (toothless without default rules) | ★★★★☆ |
| **Kani** (CBMC) | quick | arithmetic / structural invariants on bounded, total functions | proves a property over *every* in-range input, not a sample | ★★★☆☆ |
| **TLA+ / TLC** | full | spec-level concurrency / protocol errors; vacuous invariants | model-checks the protocol's reachable states; the spec is generated from the code | ★★★☆☆ |
| **TSAN** | full | real-thread data races (UB) | the one axis Loom can't model — the production threading stack | ★★★☆☆ |
| **cargo-fuzz** | full | panics/crashes on arbitrary raw input | coverage-guided bytes into parsers/decoders; a crash persists as a replay | ★★★☆☆ |
| **cucumber** (BDD) | quick | HTTP-/externally-observable behaviour | executable acceptance criteria; the human-readable traceability anchor | ★★★☆☆ |
| **Miri** | full | memory UB (invalid transmute, OOB, UAF) | interprets under a UB-detecting machine; insurance for any `unsafe` | ★★☆☆☆ |
| **Verus** (Z3) | full | unbounded ∀ / nonlinear postconditions | the deductive twin of Kani, where bounded checking can't close it | ★★☆☆☆ |
| **machete / outdated / geiger** | quick/full | unused deps; stale deps; unsafe-surface trend | hygiene; `outdated`/`geiger` advise, never block | ★★ |
| `#![forbid(unsafe_code)]` | compile | any `unsafe` in shipped crates | a compile error, stronger than any audit | n/a |

What follows is one example per tool: a snippet, and the error it catches.

### clippy — the always-on floor
Four lint levels (`all + pedantic + nursery + cargo`) plus a hand-picked
restriction allowlist. It refuses whole bug classes at zero marginal cost.

```rust
let mut total = 0u32;
for t in todos { total += weight(t.priority); }   // overflow on a large input
```
```
error: arithmetic operation that can overflow
  = note: `-D clippy::arithmetic_side_effects`
```
The fix is `fold(0, u32::saturating_add)`. The same restriction set rejects a
panicking `OffsetDateTime - OffsetDateTime`, an unchecked `[i]`, a stray
`.unwrap()`, and a lossy `as` cast — each before a single test runs.

### proptest — laws over a thousand inputs
Assert a *law*, not an example; on failure it shrinks to the minimal case.

```rust
proptest! {
    #[test]
    fn due_in_days_is_floor(now in any_ts(), due in any_ts()) {
        let d = due_in_days(now, due);
        prop_assert!(d * DAY <= secs(due) - secs(now));   // floor, not truncation
    }
}
```
```
Test failed: assertion failed. minimal failing input: now=.., due=now-12h
```
An item overdue by 12 h reported `0` ("on time") — truncation toward zero, not
floor. The example test only checked positive exact days and missed it.

### Kani — proof over every bounded input
Symbolic inputs (`kani::any()`), explored exhaustively within bounds.

```rust
#[kani::proof]
fn weight_is_bounded() {
    let p: Priority = kani::any();
    let w = weight(p);
    assert!(w >= 1 && w <= MAX_WEIGHT);   // ∀ priority, not a sample
}
```
```
SUMMARY: FAILED — assertion failed: w <= MAX_WEIGHT
  (a new Priority variant was added without updating MAX_WEIGHT)
```

### Verus — proof over *all* integers
Where the property is unbounded or nonlinear, the deductive (SMT) twin of Kani:

```rust
proof fn ceil_div_is_tight(n: nat, d: nat)
    requires d > 0,
    ensures  ceil_div(n, d) * d >= n,        // sufficient …
             (ceil_div(n, d) - 1) * d < n,   // … and tight, for ALL n, d
{ }
```
Kani can only check a bounded slice of `n`; Verus closes the ∀.

### TLA+ / TLC — model-check the protocol
You write the *specification* of a state machine or concurrency protocol; TLC
explores **all** reachable states for an invariant violation. For the lifecycle
FSM you don't hand-write it — `dhx regen` generates it from `state.rs::next`, so
the model checks the same transition table the code runs.

```tla
\* Archived is terminal: no event leaves it (REQ-001).
ArchivedTerminal == (state = "Archived" => ~ ENABLED Next)
```
```
Error: Invariant ArchivedTerminal is violated.
  state = "Archived" -> "Active"   \* a Reopen arm was wrongly allowed
```
TLC prints the exact counterexample trace — the interleaving no unit test would
have generated. And every invariant carries a mutation in `mutations.toml` that
TLC is *required* to catch, so the invariant can't be vacuous:

```toml
[[mutations]]              # flip the guard; TLC MUST report ArchivedTerminal
find    = "(state = \"Archived\" => ~ ENABLED Next)"
replace = "(state = \"Archived\" => ENABLED Next)"
expect  = "ArchivedTerminal"
```

### DST — the real app, simulated and replayable
Deterministic Simulation Testing runs the actual service over a `turmoil`
network with a mocked clock and a seeded shadow oracle, driving multi-step
sequences and faults.

```sh
dhx dst --seed 42 --iterations 20000     # reproducible regression
dhx dst --seed random                    # discovery; prints the seed to replay
```
```
DST FAILED (seed=42): after create→patch→delete, version went 1 → 1 (expected 2)
  reproduce: dhx dst --seed 42
```
A version-arithmetic bug visible only across a *sequence* of operations — found
by entropy, then pinned forever by its seed.

### Loom — every thread interleaving
Exhaustively enumerates the schedules of a shared-memory pattern.

```rust
loom::model(|| {
    let n = Arc::new(AtomicU32::new(0));
    // two threads each do read-modify-write
    assert_eq!(final_value, 2);   // holds under EVERY interleaving?
});
```
```
thread panicked: assertion failed (a lost update under one schedule)
```
A non-atomic counter loses an update on exactly one of the interleavings Loom
tries — invisible to a single-threaded test, which only ever sees one schedule.

### TSAN — races on the real threading stack
What Loom can't model (the production runtime). Recompiles std with the thread
sanitizer.

```
WARNING: ThreadSanitizer: data race
  Write of size 4 by thread T2 ... in core::ptr::write
```
A non-atomic shared write *usually* produces the right total on a quiet machine
(so the test is green and ships); TSAN reports the race regardless of schedule.

### Miri — undefined behaviour
Interprets the program under a machine that detects UB.

```rust
let p: Priority = unsafe { std::mem::transmute(7u8) };   // 7 is not a variant
```
```
error: Undefined Behavior: constructing an invalid value:
  encountered 0x07, but expected a valid enum tag
```
Green under `cargo build` and the happy-path test; only a UB checker sees it.

### cargo-fuzz — panics on arbitrary input
Coverage-guided mutated bytes into a parser; a crash is saved as a replay.

```rust
fuzz_target!(|data: &str| { let _ = parse_bulk_import(data); });
```
```
thread panicked: 'attempt to subtract with overflow'   (input: "\x0e")
  Test unit written to fuzz/artifacts/.../crash-3203...
```
A hand-indexed parser underflows on a leading control byte — found in under a
second.

### cargo-mutants — does the suite have teeth?
Mutates the *product* code and reruns the tests; a surviving mutant is a test
that doesn't actually check that behaviour.

```
MISSED: replace `>` with `>=` in rate_limit.rs:42 — caught by 0 tests
```
The boundary `n == capacity` was never tested. Coverage was green; the mutant
proved the test was hollow. Pin it with a `n == capacity` test.

### cucumber (BDD) — executable acceptance criteria
A requirement criterion, written in EARS-shaped Gherkin, run as a test. The
human-readable anchor that `check-bdd-coverage` ties back to each `REQ-NNN`.

```gherkin
Scenario: REQ-042 — over-budget create is rejected
  Given a user at their per-second create limit
  When the user POSTs another todo
  Then the service shall respond 429 with a Retry-After header
```
```
Step failed: expected status 429, got 200   (the limiter wasn't wired in)
```
The behaviour is stated the way the requirement is, so a missing criterion is a
failing scenario — and `check-bdd-coverage` fails if a criterion has no
scenario (or a `(verified=…)` marker backed by a real proof link).

### cargo-deny / gitleaks / machete — the supply-chain & secret floor
```
error[rejected]: failed to satisfy license requirements
  license = "WTFPL"   rejected: not explicitly allowed   (terminfo v0.9.0)
```
```
gitleaks: leaked credential — generic-api-key at src/config.rs:12
```
cargo-deny rejects a non-allowlisted license or a RUSTSEC advisory; gitleaks
catches a committed secret (it is *silently toothless* without `useDefault =
true` — a real incident the harness fixed); machete flags a dependency you
declared but never used. None of these is visible to compilation.

---

## Design principles (hard-won)

- **No silently-toothless gate.** Input present but unconfigured ⇒ fail loudly,
  never skip. This is the project's defining invariant.
- **Pins are the single version authority.** The Docker image is built *from*
  `.harness/pins/*`; there is no second source to drift.
- **One tool per bug class; route, don't spray.** A cheap always-on floor plus
  heavy instruments aimed where the bug-surface is.
- **The verifier verifies itself** with the universal subset of its own gates.

---

## Working on `dhx` itself

See [CLAUDE.md](CLAUDE.md) — the invariants for developing the tool (it dogfoods
its own gates: fmt, clippy, tests, every file ≤ 400 lines), the module layout,
and the conventions (`&Config` everywhere, one `corpus` walk, presence ⇒
mandatory, inert embedded assets).

## License

Dual MIT / Apache-2.0.

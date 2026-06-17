# Deterministic Harness (`dhx`)

Code is cheap to write now and expensive to trust. The bottleneck has moved from
*writing* to *verifying* — and "it compiles and the tests pass" was never the
same thing as "it is correct."

`dhx` is the answer that bets on the harness, not the model: it scaffolds a new
Rust service whose architecture is built so that a deep stack of verifiers —
type checking, property tests, model checking, bounded and deductive proofs,
simulation, race detectors — can each have *teeth*, and one CLI runs them
locally. There is no CI; the gates **are** the gate.

It is a scaffolder, not a linter you point at an existing repo. The gates only
bite because the project has a particular shape (an IO-free core behind
Clock/Rng/IdGen ports); `dhx init` lays that shape down so the harness is
meaningful from the first commit. The rest of this document is the argument for
why that trade is worth making, and a tour of every tool — with the bug each one
catches.

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

An agent is told to fix a panic in a handler. It reads the file, finds a missing
check, adds it, runs the tests. They pass. It commits, and the task is closed.
Three days later a different code path panics in production: the fix had shadowed
a legitimate error case. No test exercised that path, the diff was three lines,
and review caught nothing. By every measure available to the loop it ran in, the
change was a success — and it was wrong.

That gap is the whole problem. For most of software's history, *writing* code was
the expensive part and verifying it rode along for free in the act of a human
typing it slowly and reading it back. That economics has inverted. Code is now
cheap to produce — by a model, or by a hurried human leaning on one — and the
scarce resource is the confidence that it does what was meant. "It compiles and
the tests pass" was always a proxy for correctness, and a leaky one; at machine
speed the leaks become the dominant cost. A larger model does not close the gap,
because the gap is not in the writing. It is in the checking.

### The lever is the harness, not the model

So this project makes a bet that a growing number of teams — at Anthropic,
Datadog, AWS, Sourcegraph, and elsewhere — have arrived at independently: the
thing worth investing in is not the author but the *harness* around it. The
author proposes; a deterministic stack of verifiers disposes. Datadog put the
loop in one line — *the agent generates code, the harness verifies it,
production telemetry validates it* — and the principle that falls out of it
governs everything below: **verifiability bounds what can be created.** Wherever
a property can be checked automatically, the work behind it can be delegated and
trusted; wherever it cannot, it cannot, no matter how fluent the author.

This is a real shift in where trust lives. We trust a *compiler* because its
input language has precise semantics and its translation to machine code is
well-defined — correctness flows from the formality of the input. An author
working in natural language offers no such contract: nothing guarantees the
output corresponds to the intent. So the trust boundary moves downstream, from
the author to the *oracle* — the external checker that runs after the edit and
that the author must satisfy. The model can be brilliant or mediocre; what makes
its output trustworthy is that an oracle it does not control has signed off.

### Determinism is the precondition

An oracle is only an oracle if it returns the **same verdict for the same input,
every time.** A gate that is green on Tuesday and red on Wednesday for no reason
teaches everyone to stop reading it, and a checker nobody reads is worse than no
checker at all. So determinism is not a nice-to-have here; it is the thing that
makes the whole arrangement load-bearing.

The harness pursues it on three fronts. Inside the program, every source of
ambient non-determinism — the wall clock, randomness, identifier generation —
is forced through a *port* (`Clock`, `Rng`, `IdGen`), so that a single seed
fully determines a run and any failure can be replayed exactly; `clippy.toml`
bans the direct calls so the discipline cannot quietly erode. Across the
toolchain, the few gates that *use* entropy on purpose — fuzzing, property
testing, random-seed simulation — are discovery tools that persist whatever they
find, so a bug discovered by chance becomes a fixed, replayable regression.
Across machines, every external tool's version is pinned, and `verify --full`
runs inside a Docker image built *from those pins*, so two developers on two
laptops reach one verdict.

### Gates must have teeth

The failure mode that matters most in a verification project is not a gate that
is too strict. It is the **silently toothless gate**: the one that reports green
while checking nothing. A secret scanner shipped with no rules. A model-checker
invariant that happens to constrain nothing. A coverage gate pointed at the
wrong crate. Each of these is worse than having no gate, because it manufactures
confidence where there is none — and that false confidence is exactly what let
the agent in the opening story close its task.

The harness is engineered against this one mistake. Its central rule is
*presence implies mandatory*: if a project **looks** as though it has something
to verify — an FSM source on disk, a TLA⁺ module that declares invariants, a
requirement that claims it is `verified=`-by-proof — but that thing is not
actually wired into a gate, `dhx` fails loudly rather than skipping in silence.
"Out of scope" is something you must declare and can audit; "absent by accident"
must never read as "passing." The same instinct is applied recursively: every
TLA⁺ invariant must carry a known-violating mutation the checker is *required*
to catch, so an invariant cannot ship vacuous; mutation testing sits behind the
coverage number to prove the tests actually kill bugs rather than merely
executing lines; and `dhx` runs the cheap subset of its own gates against its
own source, because an unverified verifier would be the most toothless gate of
all.

### Comprehensive, but routed

Once you have a stack of oracles, the temptation is to run all of them on
everything. That is ceremony, and ceremony is waste. The skill the pipeline
actually encodes is *routing*: matching each tool to the question it is good at,
and spending the expensive ones only where a feature's hardest question lives. A
useful way to hold the stack in mind is as a pyramid — slowest and most about
*understanding* at the top, most *diagnostic* at the base: a TLA⁺ spec to reason
about a concurrent protocol; deterministic simulation as the primary integration
test; bounded and deductive proofs (Kani, Verus) for the pure core; and, at the
bottom, empirical ground truth from benchmarks and telemetry. Underneath all of
it, on every edit at nearly no cost, runs the floor — clippy, property tests,
and the intent-drift meta-gates.

The hard-won finding behind this shape is that **payoff tracks a feature's
bug-surface, not the effort spent on it.** The harness earns its keep
decisively on arithmetic, boundaries, dates, concurrency, undefined behaviour,
and untrusted input — and is honest dead weight on flat CRUD. Knowing which is
which is the entire craft.

### The shape that gives the gates teeth

None of this is free, and the price is an opinion about how the project is laid
out. The gates are not generic linters that work anywhere; they verify a
specific architecture, and that architecture is precisely what lets them bite —
which is why `dhx` is a scaffolder rather than something you point at a repo that
already exists.

The core is a pure, IO-free crate: no database, no HTTP, no async runtime in
`crates/core`. A pure function has no hidden inputs, and that is exactly what
lets Kani and Verus prove it total and lets property tests assert laws over it.
All the messiness — clocks, randomness, the network, the database — lives behind
the ports, in outer crates, which is the seam that makes simulation, Loom, and
the race detectors deterministic and replayable. And the project is a workspace,
so coverage can run the whole test suite while reporting only on the core,
counting every integration test that exercises it. Persistence and HTTP — axum,
sqlx, whatever you reach for — are optional layers behind the ports, never a
requirement of the harness; the harness needs the architecture, not any
particular library.

This is offered as the right shape for the current era of agentic development,
and stated honestly as that — not a claim for all time. The harness is built to
be evolved.

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

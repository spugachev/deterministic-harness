# Deterministic Progressive Hardening: Building Reliable Agentic Harness

Code is cheap to write now and expensive to trust. The bottleneck has moved from
_writing_ to _verifying_ — and "it compiles and the tests pass" was never the
same thing as "it is correct."

`dhx` is the answer that bets on the harness, not the model: it scaffolds a new
Rust service whose architecture is built so that a deep stack of verifiers —
type checking, property tests, model checking, bounded and deductive proofs,
simulation, race detectors — can each catch the bug it is responsible for, and
one CLI runs them all locally.

---

## How

Build the image once; run everything — scaffolding included — through it. There
is no `dhx` on your host: the single `dhx:latest` image bakes the CLI plus every
pinned tool, so a gate can never run against an unpinned host version.

```sh
# 1. build the one image (it holds dhx + every tool, at the pinned versions)
git clone https://github.com/spugachev/deterministic-harness
cd deterministic-harness && docker build -t dhx:latest .

# 2. define dhx as a shell function — it mounts CACHE VOLUMES so iteration is fast
#    (the crate registry downloads once into a shared volume; target/ is a
#    per-project volume, so the second run onward only recompiles what changed):
dhx() { docker run --rm -v "$PWD":/work -w /work \
          -v dhx-cargo-registry:/root/.cargo/registry \
          -v "dhx-target-$(basename "$PWD")":/work/target \
          dhx:latest dhx "$@"; }

# 3. scaffold a new service, then run every tier as if dhx were local
mkdir ~/code/payments-svc && cd ~/code/payments-svc
dhx init .
dhx check           # cheap gates
dhx verify --quick  # + tests, proptest, coverage, Kani, TLA+/TLC, 1 DST seed
dhx verify --full   # + TSAN, mutants, fuzz, Loom, multi-seed DST
```

Without the cache volumes every run recompiles the whole dependency tree from
scratch. Measured on a fresh scaffold (seats example, Apple Silicon):

| run | `verify --quick` | `check` |
| --- | --- | --- |
| cold, no volumes (worst case) | ~164 s | — |
| first run, cache volumes (registry pre-warmed in the image) | ~72 s | — |
| **warm re-run** (hot `target/` volume) | **~30 s** | **~1 s** |

The volumes are the difference between "verify continuously" and "verify
occasionally," so the function above is the recommended way to invoke dhx. (Put
it in your shell rc; `dhx-cargo-registry` is
safe to share across all projects, the per-project `dhx-target-*` is not.)

`dhx init` writes a workspace with an IO-free core, the Clock/Rng/IdGen ports, a
mandatory BDD+EARS scenario per requirement, `spec/` (requirements, ADRs, and
TLA+ once you add a state machine), `.harness/` (version pins, tool configs, git
hooks), a `clippy.toml` of determinism bans, and `CLAUDE.md` + `.claude/` (skills
`/check` `/verify`, a post-edit hook) so an agent is wired in from the first
commit. It ships a throwaway `domain::example` (REQ-001) as a green seed to
replace, and needs no Dockerfile of its own — the shared `dhx:latest` image runs
its gates.

The tiers are split by **wall-clock cost**, so the fast verifiers run constantly
and only the slow ones wait (each `dhx <cmd>` is the `docker run … dhx:latest dhx
<cmd>` invocation above):

| Tier      | Command              | When                  | Adds                                                      |
| --------- | -------------------- | --------------------- | --------------------------------------------------------- |
| Preflight | `dhx check`          | every save (~s)       | fmt, clippy, all meta-gates (incl. BDD coverage)          |
| Quick     | `dhx verify --quick` | after a small change  | + tests, proptest, coverage, Kani, **TLA+/TLC**, 1 DST seed |
| Full      | `dhx verify --full`  | after a big change    | + TSAN, mutants, fuzz, Loom, multi-seed DST               |

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

That gap is the whole problem. For most of software's history, _writing_ code was
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
thing worth investing in is not the author but the _harness_ around it. The
author proposes; a deterministic stack of verifiers disposes. Datadog put the
loop in one line — _the agent generates code, the harness verifies it,
production telemetry validates it_ — and the principle that falls out of it
governs everything below: **verifiability bounds what can be created.** Wherever
a property can be checked automatically, the work behind it can be delegated and
trusted; wherever it cannot, it cannot, no matter how fluent the author.

This is a real shift in where trust lives. We trust a _compiler_ because its
input language has precise semantics and its translation to machine code is
well-defined — correctness flows from the formality of the input. An author
working in natural language offers no such contract: nothing guarantees the
output corresponds to the intent. So the trust boundary moves downstream, from
the author to the _oracle_ — the external checker that runs after the edit and
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
is forced through a _port_ (`Clock`, `Rng`, `IdGen`), so that a single seed
fully determines a run and any failure can be replayed exactly; `clippy.toml`
bans the direct calls so the discipline cannot quietly erode. Across the
toolchain, the few gates that _use_ entropy on purpose — fuzzing, property
testing, random-seed simulation — are discovery tools that persist whatever they
find, so a bug discovered by chance becomes a fixed, replayable regression.
Across machines, every external tool's version is pinned, and _every_ tier runs
inside one Docker image built _from those pins_ — there is no host `dhx` to drift
against — so two developers on two laptops reach one verdict.

### A green gate must mean something

The failure mode that matters most in a verification project is not a gate that
is too strict. It is the **gate that reports green while checking nothing** — a
secret scanner shipped with no rules, a model-checker invariant that happens to
constrain nothing, a coverage gate pointed at the wrong crate. Each is worse
than having no gate at all, because it manufactures confidence where there is
none, and that false confidence is exactly what let the agent in the opening
story close its task.

The harness is engineered against this one mistake. Its central rule is
_presence implies mandatory_: if a project **looks** as though it has something
to verify — an FSM source on disk, a TLA+ module that declares invariants, a
requirement that claims it is `verified=`-by-proof — but that thing is not
actually wired into a gate, `dhx` fails loudly rather than skipping in silence.
"Out of scope" is something you must declare and can audit; "absent by accident"
must never read as "passing." The same instinct is applied recursively: every
TLA+ invariant carries a known-violating mutation the checker is _required_ to
report, so an invariant cannot pass while constraining nothing; mutation testing
sits behind the coverage number to confirm the tests actually fail when the
logic is broken, not merely execute lines; and `dhx` runs the cheap subset of
its own gates against its own source, because a verifier nobody verifies is the
emptiest gate of all.

### Comprehensive, but routed

Once you have a stack of oracles, the temptation is to run all of them on
everything. That is ceremony, and ceremony is waste. The skill the pipeline
actually encodes is _routing_: matching each tool to the question it is good at,
and spending the expensive ones only where a feature's hardest question lives. A
useful way to hold the stack in mind is as a pyramid — slowest and most about
_understanding_ at the top, most _diagnostic_ at the base: a TLA+ spec to reason
about a concurrent protocol; deterministic simulation as the primary integration
test; bounded proofs (Kani) for the pure core; and, at the
bottom, empirical ground truth from benchmarks and telemetry. Underneath all of
it, on every edit at nearly no cost, runs the floor — clippy, property tests,
and the intent-drift meta-gates.

The hard-won finding behind this shape is that **payoff tracks a feature's
bug-surface, not the effort spent on it.** The harness earns its keep
decisively on arithmetic, boundaries, dates, concurrency, undefined behaviour,
and untrusted input — and is honest dead weight on flat CRUD. Knowing which is
which is the entire craft.

### The architecture that makes it work

None of this is free, and the price is an opinion about how the project is laid
out. The gates are not generic linters that work anywhere; they verify a
specific architecture, and that architecture is precisely what gives each
verifier something solid to check — which is why `dhx` is a scaffolder rather
than something you point at a repo that already exists.

The core is a pure, IO-free crate: no database, no HTTP, no async runtime in
`crates/core`. A pure function has no hidden inputs, and that is exactly what
lets Kani prove it total and lets property tests assert laws over it.
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
duplicate would only add wall-clock and "which one is authoritative?" arguments
for no safety gain. The tables read top to bottom in the order a developer
should meet the tools — the cheap, broad floor first, then progressively more
specialised instruments, grouped by the question each answers. "Tier" is the
cheapest tier the tool runs in; ★ is practical bug-catching ROI (from A/B
studies and planted-defect probes on the prototype this harness came from), not
raw capability.

**Static floor — runs on every edit, ~free**

| Tool                                                                                    | Tier  | Catches                                                                                               | How it helps                                      | ★     |
| --------------------------------------------------------------------------------------- | ----- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------- | ----- |
| [**clippy**](https://doc.rust-lang.org/clippy/) (4 levels + restriction)                | check | antipatterns, unchecked arithmetic, lossy casts, reachable panics, complexity, direct non-determinism | refuses whole bug classes before a test even runs | ★★★★★ |
| **meta-gates** (traceability, spec-sync, bdd/mutation-coverage, file-size, docs-counts) | check | drift between spec, code, and docs; vacuous invariants; a gate that checks nothing                    | keep every other gate and the docs honest         | ★★★★☆ |

**Behaviour & test quality — does it do the right thing, and do the tests prove it**

| Tool                                                            | Tier  | Catches                                                                 | How it helps                                                                                                | ★     |
| --------------------------------------------------------------- | ----- | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- | ----- |
| [**cucumber**](https://github.com/cucumber-rs/cucumber) (BDD)   | quick | implemented behaviour that diverges from its stated acceptance criteria | turns each requirement into an executable, human-readable scenario, run end-to-end against the real service | ★★★☆☆ |
| [**proptest**](https://github.com/proptest-rs/proptest)         | quick | violation of a pure law (idempotence, monotonicity, round-trip, bounds) | finds the boundary the happy-path test forgot, then shrinks it                                              | ★★★★★ |
| [**cargo-llvm-cov**](https://github.com/taiki-e/cargo-llvm-cov) | quick | untested regions of the verified core                                   | the quantitative adequacy floor under the test suite                                                        | ★★★★☆ |
| [**cargo-mutants**](https://mutants.rs/)                        | full  | weak tests (logic inverted, tests still green)                          | proves the suite kills bugs, not just executes lines                                                        | ★★★★☆ |

**Proofs of the pure core — exhaustive, not sampled**

| Tool                                                      | Tier  | Catches                                                        | How it helps                                  | ★     |
| --------------------------------------------------------- | ----- | -------------------------------------------------------------- | --------------------------------------------- | ----- |
| [**Kani**](https://github.com/model-checking/kani) (CBMC) | quick | arithmetic / structural invariants on bounded, total functions | proves a property over _every_ in-range input | ★★★☆☆ |

**Concurrency & protocol — the bugs that are about _schedules_, not inputs**

| Tool                                                                                                   | Tier  | Catches                                                      | How it helps                                                                      | ★     |
| ------------------------------------------------------------------------------------------------------ | ----- | ------------------------------------------------------------ | --------------------------------------------------------------------------------- | ----- |
| [**TLA+ / TLC**](https://github.com/tlaplus/tlaplus)                                                   | quick | spec-level concurrency / protocol errors; vacuous invariants | model-checks the protocol's reachable states; for an FSM the spec is generated from the code | ★★★☆☆ |
| [**DST**](https://github.com/tokio-rs/turmoil) (turmoil)                                               | quick | full-stack multi-step / network / fault-injection sequences  | drives the real app over a simulated network; replays any failure from a seed     | ★★★★☆ |
| [**Loom**](https://github.com/tokio-rs/loom)                                                           | full  | in-memory data races / lost updates                          | exhausts every thread interleaving of a shared-memory pattern                     | ★★★★☆ |
| [**TSAN**](https://doc.rust-lang.org/beta/unstable-book/compiler-flags/sanitizer.html#threadsanitizer) | full  | real-thread data races (UB)                                  | the one axis Loom can't model — the production threading stack                    | ★★★☆☆ |

**Memory safety & untrusted input**

| Tool                                                                                                               | Tier    | Catches                               | How it helps                                                              | ★     |
| ------------------------------------------------------------------------------------------------------------------ | ------- | ------------------------------------- | ------------------------------------------------------------------------- | ----- |
| [`#![forbid(unsafe_code)]`](https://doc.rust-lang.org/reference/attributes/codegen.html#the-unsafe_code-attribute) | compile | any `unsafe` in shipped crates        | a compile error — removes memory-UB as a class, stronger than any audit   | n/a   |
| [**cargo-fuzz**](https://github.com/rust-fuzz/cargo-fuzz)                                                          | full    | panics/crashes on arbitrary raw input | coverage-guided bytes into parsers/decoders; a crash persists as a replay | ★★★☆☆ |

**Supply chain & secrets — risks compilation can't see**

| Tool                                                                                                                                                                         | Tier       | Catches                                        | How it helps                                     | ★     |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- | ---------------------------------------------- | ------------------------------------------------ | ----- |
| [**cargo-deny**](https://github.com/EmbarkStudios/cargo-deny)                                                                                                                | quick      | vulnerable / banned / bad-license dependencies | a supply-chain class the compiler never looks at | ★★★★☆ |
| [**gitleaks**](https://github.com/gitleaks/gitleaks)                                                                                                                         | quick      | committed secrets                              | catches a key before it leaves your machine      | ★★★★☆ |
| [**machete**](https://github.com/bnjbvr/cargo-machete) / [**outdated**](https://github.com/kbknapp/cargo-outdated) / [**geiger**](https://github.com/geiger-rs/cargo-geiger) | quick/full | unused deps; stale deps; unsafe-surface trend  | hygiene; `outdated`/`geiger` advise, never block | ★★    |

What follows is one example per tool — a snippet and the error it catches — in
the same grouped order.

### Static floor

**clippy** — four lint levels (`all + pedantic + nursery + cargo`) plus a
hand-picked restriction allowlist. It refuses whole bug classes at zero marginal
cost.

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
`.unwrap()`, and a lossy `as` cast — each before a single test runs. Alongside
it, the **meta-gates** check that the spec, the code, and the docs still agree
(see "A green gate must mean something").

### Behaviour & test quality

**cucumber (BDD)** — each requirement's acceptance criterion, written in
EARS-shaped Gherkin and run end-to-end as a test against the real service. It is
how you state and check _behaviour_ — what the system should do, in the
requirement's own words — not merely an HTTP assertion.

```gherkin
Scenario: REQ-042 — over-budget create is rejected
  Given a user at their per-second create limit
  When the user POSTs another todo
  Then the service shall respond 429 with a Retry-After header
```

```
Step failed: expected status 429, got 200   (the limiter wasn't wired in)
```

Because the scenario is phrased like the requirement, a missing criterion _is_ a
failing or absent scenario — and `check-bdd-coverage` fails if a criterion has
no scenario (or a `(verified=…)` marker backed by a real proof link).

**proptest** — assert a _law_, not an example; on failure it shrinks to the
minimal case.

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

**cargo-llvm-cov** sets the coverage floor on the core; **cargo-mutants** then
confirms those tests actually catch bugs — it mutates the _product_ code and
reruns the suite, and a surviving mutant is a test that never really checked
that behaviour.

```
MISSED: replace `>` with `>=` in rate_limit.rs:42 — caught by 0 tests
```

The boundary `n == capacity` was never tested. Coverage was green; the mutant
proved the test was hollow. Pin it with a `n == capacity` test.

### Proofs of the pure core

**Kani** — symbolic inputs (`kani::any()`), explored exhaustively within bounds.

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

Kani checks the property over _every_ in-range input within its bounds, so a
counterexample is a real one — not a sample the property test happened to miss.

### Concurrency & protocol

**TLA+ / TLC** — you write the _specification_ of a state machine or concurrency
protocol; TLC explores **all** reachable states for an invariant violation. For
the lifecycle FSM you don't hand-write it — `dhx regen` generates it from
`state.rs::next`, so the model checks the same transition table the code runs.

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
TLC is _required_ to catch, so the invariant can't be vacuous:

```toml
[[mutations]]              # flip the guard; TLC MUST report ArchivedTerminal
find    = "(state = \"Archived\" => ~ ENABLED Next)"
replace = "(state = \"Archived\" => ENABLED Next)"
expect  = "ArchivedTerminal"
```

**DST** — Deterministic Simulation Testing runs the actual service over a
`turmoil` network with a mocked clock and a seeded shadow oracle, driving
multi-step sequences and faults.

```sh
dhx dst --seed 42 --iterations 20000     # reproducible regression
dhx dst --seed random                    # discovery; prints the seed to replay
```

```
DST FAILED (seed=42): after create→patch→delete, version went 1 → 1 (expected 2)
  reproduce: dhx dst --seed 42
```

A version-arithmetic bug visible only across a _sequence_ of operations — found
by entropy, then pinned forever by its seed.

**Loom** — exhaustively enumerates the thread schedules of a shared-memory
pattern.

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

**TSAN** — what Loom can't model (the production runtime). Recompiles std with
the thread sanitizer.

```
WARNING: ThreadSanitizer: data race
  Write of size 4 by thread T2 ... in core::ptr::write
```

A non-atomic shared write _usually_ produces the right total on a quiet machine
(so the test is green and ships); TSAN reports the race regardless of schedule.

### Memory safety & untrusted input

**`#![forbid(unsafe_code)]`** — memory UB (invalid transmute, OOB, use-after-free)
can only arise through `unsafe`, so the scaffold forbids it outright in shipped
crates. This removes the entire bug class at compile time:

```rust
let p: Priority = unsafe { std::mem::transmute(7u8) };   // 7 is not a variant
```

```
error: usage of an `unsafe` block
  = note: `#[forbid(unsafe_code)]` on by default
```

A compile error is stronger than any after-the-fact UB checker (e.g. Miri), and
needs no extra gate or runtime. If a project genuinely needs `unsafe` (FFI, a
custom data structure), lift the `forbid` only in that crate and add a Miri gate
there — but the default, and the right answer for almost all domain code, is to
keep the door shut.

**cargo-fuzz** — coverage-guided mutated bytes into a parser; a crash is saved
as a replay.

```rust
fuzz_target!(|data: &str| { let _ = parse_bulk_import(data); });
```

```
thread panicked: 'attempt to subtract with overflow'   (input: "\x0e")
  Test unit written to fuzz/artifacts/.../crash-3203...
```

A hand-indexed parser underflows on a leading control byte — found in under a
second.

### Supply chain & secrets

**cargo-deny** rejects a non-allowlisted license or a RUSTSEC advisory;
**gitleaks** catches a committed secret (it scans with the default ruleset — a
config that omits it quietly finds nothing, a real incident the harness fixed);
**machete** flags a dependency you declared but never used. None of these is
visible to compilation.

```
error[rejected]: failed to satisfy license requirements
  license = "WTFPL"   rejected: not explicitly allowed   (terminfo v0.9.0)
```

```
gitleaks: leaked credential — generic-api-key at src/config.rs:12
```

---

## Does it actually catch bugs? An A/B run

To check the harness earns its keep rather than just feeling rigorous, the same
five features were built twice by the same agent (`claude -p`): once **OFF** — a
plain `cargo` library, "make it compile, one smoke test, ship it" — and once
**ON** — a `dhx init` project run through the full workflow (REQ → implement →
tests + a property law → `dhx check` green). Each OFF arm's latent bug was then
found _empirically_, by running one hostile input the smoke test skipped.

| Feature                 | OFF shipped                 | Smoke | The bug (found by probing)                    | ON shipped                 | Caught by           |
| ----------------------- | --------------------------- | ----- | --------------------------------------------- | -------------------------- | ------------------- |
| `workload(&[u32])` sum  | `iter().sum()`              | ✅    | `[u32::MAX; 2]` → **overflow panic**          | `fold(0, saturating_add)`  | clippy + proptest   |
| `blend(a,b,wa)` average | `100 - wa`                  | ✅    | `wa = 200` → **subtract-overflow panic**      | clamped `saturating_sub`   | clippy + proptest   |
| `due_in_days`           | `(due-now)/86400`           | ✅    | overdue 12 h → **returns `0`, not `-1`**      | `div_euclid` (floor)       | proptest            |
| `parse_line`            | `chars().next()` / `splitn` | ✅    | `"noseparator"` → **silently drops the text** | `-> Result<_, ParseError>` | proptest + the type |
| `RingBuf` fixed cap     | correct                     | ✅    | _none_                                        | same + invariant           | — (fair tie)        |

**OFF shipped 4 real defects across 5 features; the smoke tests caught 0 of 4.
ON shipped none**, each reaching a green `dhx check`. The wins were exactly where
theory predicts — arithmetic, a boundary, a date sign, a parse — and the fifth
feature (a plain ring buffer, no boundary as its hardest question) was an honest
tie. The harness did not make the agent smarter; it changed what counts as
"done," and that bar is what forced `saturating_add`, `div_euclid`, clamping,
and a `Result`.

---

## Design principles (hard-won)

- **A green gate must mean something.** Input present but unconfigured ⇒ fail
  loudly, never skip. This is the project's defining invariant.
- **Pins are the single version authority.** The one `dhx:latest` image is built
  _from_ the scaffold's `.harness/pins/*`; there is no second source to drift,
  and no host `dhx` to run gates against unpinned tools.
- **One tool per bug class; route, don't spray.** A cheap always-on floor plus
  heavy instruments aimed where the bug-surface is.
- **The verifier verifies itself** with the universal subset of its own gates.

---

## Working on `dhx` itself

See [CLAUDE.md](CLAUDE.md) — the invariants for developing the tool (it dogfoods
its own gates: fmt, clippy, tests, every file ≤ 400 lines), the module layout,
and the conventions (`&Config` everywhere, one `corpus` walk, presence ⇒
mandatory, inert embedded assets).

## Further reading

The "harness over model" thesis is not unique to this project; it is a
convergence several teams reached independently. The works that most directly
shaped the approach here:

- Datadog — [_Closing the verification loop: observability-driven harnesses for
  building with agents_](https://www.datadoghq.com/blog/ai/harness-first-agents/)
  — the source of "the agent generates code, the harness verifies it, telemetry
  validates it," and of the verification pyramid (TLA+ → DST → Kani → telemetry).
- Datadog — [_Closing the verification loop, Part 2: fully autonomous
  optimization_](https://www.datadoghq.com/blog/ai/fully-autonomous-optimization/)
  — a verifier-gated pipeline (Verus proofs + sandbox + shadow eval) running with
  no human in the loop.
- Jerome Van Der Linden — [_No reliable autonomy without determinism:
  building guardrails for autonomous software
  agents_](https://builder.aws.com/content/3CuKqf5cM4vSShhg5lPErnUmlA4/no-reliable-autonomy-without-determinism-building-guardrails-for-autonomous-software-agents)
  — determinism as the precondition for delegating to an agent.
- Brooker & Desai — [_Systems correctness practices at Amazon Web
  Services_](https://cacm.acm.org/practice/systems-correctness-practices-at-amazon-web-services/),
  _CACM_ 2025 — formal methods (TLA+, [P](https://github.com/p-org/P), DST) as
  routine engineering practice at scale.
- Anthropic — [_Building effective
  agents_](https://www.anthropic.com/engineering/building-effective-agents) —
  programmatic gates between steps; "invest in the agent-computer interface."
- TigerBeetle — [_Deterministic simulation
  testing_](https://docs.tigerbeetle.com/concepts/safety/) — the DST lineage the
  `turmoil`-based gate descends from.

## License

Dual MIT / Apache-2.0.

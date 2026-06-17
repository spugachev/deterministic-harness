# Philosophy — why a deterministic harness

> "The agent writes code faster than humans can review it. Every piece of code
> compiles. Every piece passes unit tests. A large fraction of the code is
> wrong."

That is the problem `dhx` exists to address. As code generation gets cheaper —
by an LLM or a hurried human — the bottleneck moves from *writing* to
*verifying*. A bigger model does not fix this; a tighter harness does.

## Oracle trust, not compiler trust

We trust a compiler because the source language has precise semantics and the
transformation to machine code is well-defined. An agent has neither: it ingests
unrestricted natural language and emits code with no contract that the output
matches the intent. So the trust boundary moves.

- **Compiler trust:** the output is correct because the input is formal.
- **Oracle trust:** the output is checked, *after the edit*, by external
  verifiers — type-checker, linter, tests, property checks, model checkers,
  proofs — and the author iterates against their verdict. **The oracle is the
  trust boundary, not the author.**

Datadog's framing of the same loop: *the agent generates code, the harness
verifies it, production telemetry validates it.* And the consequence that drives
every design decision here: **verifiability bounds what can be created** —
wherever a property can be checked automatically, more of the work can be
delegated; wherever it cannot, it cannot.

## Determinism is the prerequisite, not the goal

A verifier is only an oracle if it gives the **same verdict for the same input,
every time**. A flaky gate teaches you to ignore it. So `dhx` treats
determinism as a precondition:

- Every gate is deterministic *as a verdict*. Discovery gates (fuzz, proptest,
  random-seed DST) use entropy to *find* new bugs, but a found failure is
  persisted and replays deterministically.
- All ambient non-determinism in the domain — wall-clock, RNG, id generation —
  flows through **ports** (`Clock`/`Rng`/`IdGen`), so a test or a simulation
  fully determines a run. "Deterministic seeds make failures reproducible: the
  agent replays the exact sequence and traces the invariant violation."
- Tool versions are **pinned** (`.harness/pins/`), and the `--full` tier runs in
  a Docker image built *from those pins*, so two machines reach the same verdict.

This is not safety-critical determinism. It is *enough* determinism that a
failure is reproducible and a green run means something.

## Gates with teeth — and the worst failure mode

A gate that cannot fail is decoration. The single worst outcome in a
verification project is the **silently toothless gate**: one that reports green
while checking nothing (a secret scanner with no rules; a model-checker
invariant that constrains nothing; a coverage gate scoped at the wrong crate).
It is worse than no gate, because it manufactures false confidence.

`dhx` is built against that failure mode:

- **Presence ⇒ mandatory.** If a project *looks* like it has something to verify
  (an FSM source exists, a `.tla` declares invariants, a REQ declares a
  `verified=` claim) but it is not configured, `dhx` **fails loudly** — it never
  silently skips. "Declared out of scope" is auditable; "happened to be absent"
  is not.
- **Anti-vacuity is itself gated.** Every TLA+ invariant must have a
  known-violating mutation that the model checker is *required* to catch
  (`mutations.toml` + `check-mutation-coverage`), so an invariant cannot ship
  vacuous.
- **Test adequacy is gated.** Coverage sets a floor; `cargo-mutants` proves the
  tests actually *kill* logic mutations — a test that still passes with the
  logic inverted is a weak test and fails.
- **The verifier verifies itself.** `dhx` runs the universal subset of its own
  gates on its own source (fmt, clippy, tests, ≤400-line files, deny, machete).
  An unverified verifier would be the ultimate toothless gate.

## Harness-first, and the verification pyramid

Run the layer that answers the question you are asking — do not run all of them
on every commit. The layers, slowest/most-understanding at the top, most
diagnostic at the bottom:

1. **TLA+** — *understanding* of a concurrent protocol; the FSM spec is
   generated from the code, the concurrent spec is hand-written.
2. **DST** — the primary integration test: the real app over a simulated
   network with a mocked clock, seeded and replayable.
3. **Kani / Verus** — bounded and deductive proofs of pure functions.
4. **Telemetry / benchmarks** — empirical ground truth.

The everyday floor underneath all of it is clippy + proptest + the cheap
intent-drift meta-gates — they run on every edit at near-zero cost. The heavy
instruments are aimed, not sprayed.

The decisive empirical finding behind this (from three A/B studies on the
prototype this harness came from): **payoff tracks a feature's bug-surface, not
effort.** The harness is decisively worth it where the hardest question is
arithmetic, a boundary, a date, concurrency, UB, or untrusted input — and dead
weight on flat CRUD. The skill the pipeline encodes is *routing*. See
[toolchain.md](toolchain.md) and [workflow.md](workflow.md).

## Scope, honestly stated

`dhx` is an **opinionated scaffolder**, not a linter you point at an arbitrary
repo. The gates have teeth *because* the project has the shape in
[architecture.md](architecture.md). Pointed at a project without that shape, the
gates would be either red-on-arrival or verifying nothing — so that use is out
of scope, by design. This is the right answer for the current decade of agentic
development, not a claim for all time; the harness is meant to be evolved.

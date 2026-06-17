# Deterministic Harness (`dhx`)

An **opinionated scaffolder** that creates a new Rust service already wired into
a comprehensive verification toolchain, and a single CLI (`dhx`) that runs every
gate locally — there is no CI.

- **Comprehensive** — the toolchain covers every bug class: style/arithmetic
  (clippy), pure laws (proptest), bounded/deductive proofs (Kani/Verus),
  concurrency & protocol (TLA+/Loom/TSAN/DST), UB (Miri), raw-input panics
  (fuzz), weak tests (mutation), supply chain (deny/machete), and intent drift
  (the traceability/spec-sync/docs/mutation-coverage meta-gates).
- **Universal** — it applies to the whole *class* of verified services of the
  scaffold's shape (a workspace with an IO-free core behind Clock/Rng/IdGen
  ports). The opinionated architecture is what lets all the gates have teeth at
  once. It is **not** a linter you point at an arbitrary existing repo.

## Quickstart — from clone to a verified, Claude-wired project

```sh
# 1. Get dhx and install the CLI (one time)
git clone https://github.com/spugachev/deterministic-harness deterministic-harness
cd deterministic-harness
cargo install --path dhx          # `dhx` now on PATH

# 2. Scaffold a NEW project anywhere on disk
dhx init ~/code/payments-svc      # arbitrary path; creates + git-inits it
cd ~/code/payments-svc

# 3. It already IS a deterministic-harness service:
dhx check                         # cheap gates — green out of the box
dhx verify --quick                # + tests, coverage, Kani, DST (host)
dhx verify --full                 # everything, in the pinned Docker image

# 4. Claude Code is wired in automatically (step 2 wrote .claude/ + CLAUDE.md):
#    - CLAUDE.md loads the conventions + the architecture precondition
#    - /check and /verify skills are available
#    - a PostToolUse hook runs fmt+check after every .rs edit
```

The scaffolded project has **no path dependency** on this repo — `dhx` is found
on `PATH`, and `harness.toml` is self-contained.

## What `dhx init` produces

```
my-svc/
├── harness.toml          the one manifest dhx reads (gates, targets, pins, fsm)
├── .harness/             pins/, config/ (relocated deny/gitleaks/mutants/nextest), hooks/
├── spec/                 requirements/ (REQ-NNN), adr/, tla/ (+ mutations.toml)
├── crates/core/          the IO-free verified core: domain/ (the FSM) + ports/
├── clippy.toml           the determinism bans (the architecture precondition)
├── Dockerfile            built from .harness/pins/* — the --full image
└── CLAUDE.md + .claude/  conventions, /check + /verify skills, post-edit hook
```

## The gates (tiers)

| Tier | Command | Runs |
|---|---|---|
| Preflight | `dhx check` | 11 cheap deterministic gates, all failures aggregated |
| Quick | `dhx verify --quick` | + test, coverage, kani, dst |
| Full | `dhx verify --full` | + verus, miri, tsan, loom, fuzz, mutants, tlc(+mutate) — **in Docker** |

`dhx config explain <gate>` shows a gate's resolved value and where it came from.

## The tools — what each does, what it catches, what it's worth

Ratings are practical bug-catching ROI (graded from A/B studies + planted-defect
probes on the prototype this harness came from), not raw capability. Full
detail, runnable commands, and the bug-class → tool reverse lookup are in
[docs/toolchain.md](docs/toolchain.md).

| Tool | Tier | Catches | Usefulness |
|---|---|---|---|
| **clippy** (4 levels + restriction) | check | antipatterns, unchecked arithmetic, lossy casts, reachable panics, complexity, direct non-determinism | ★★★★★ best ROI; the always-on floor |
| **proptest** | quick | violations of a pure law (idempotence/monotonicity/round-trip/bounds) | ★★★★★ broad, cheap workhorse |
| **meta-gates** (traceability, spec-sync, bdd, mutation-coverage, file-size, docs-counts) | check | spec ↔ code ↔ docs drift; vacuous invariants; the *toothless gate* itself | ★★★★☆ cheap, keeps everything else honest |
| **DST** (turmoil) | quick | full-stack multi-step / network / fault-injection sequences | ★★★★☆ unique for sequences; replayable |
| **cargo-mutants** | full | weak tests (logic-inverted-still-passes) | ★★★★☆ unique; proves test adequacy |
| **Loom** | full | in-memory races / lost updates (schedules, not inputs) | ★★★★☆ narrow but unique |
| **cargo-deny** | quick | vulnerable / banned / bad-license deps | ★★★★☆ a class compilation can't see |
| **cargo-llvm-cov** | quick | untested regions of the verified core | ★★★★☆ adequacy floor |
| **gitleaks** | quick | committed secrets | ★★★★☆ (toothless without `useDefault`) |
| **Kani** | quick | bounded arithmetic/structural invariants (∀ in-range) | ★★★☆☆ high value, some operational care |
| **TLA+/TLC** | full | spec-level concurrency / protocol errors | ★★★☆☆ only when concurrent — then unique |
| **TSAN** | full | real-thread data races (UB) | ★★★☆☆ insurance until shared state appears |
| **cargo-fuzz** | full | raw-input panics (parsers/decoders) | ★★★☆☆ high on untrusted input, low elsewhere |
| **cucumber** (BDD) | quick | HTTP-/externally-observable behaviour | ★★★☆☆ readability + traceability anchor |
| **Miri** | full | memory UB (transmute/OOB/UAF) | ★★☆☆☆ insurance while `forbid(unsafe)` holds |
| **Verus** | full | unbounded ∀ / nonlinear postconditions | ★★☆☆☆ deductive contrast to Kani |
| **cargo-machete / outdated / geiger** | quick/full | unused deps; stale deps; unsafe-surface trend | ★★–★★★ hygiene; outdated/geiger never block |

The guiding rule is **one tool per bug class** and **route, don't spray**: a
cheap always-on floor plus heavy instruments aimed where the feature's hardest
question lands. The decisive finding: *payoff tracks a feature's bug-surface,
not effort.*

## Documentation

A coherent manual lives in [docs/](docs/):

- [docs/philosophy.md](docs/philosophy.md) — oracle trust vs compiler trust, why determinism, gates with teeth.
- [docs/architecture.md](docs/architecture.md) — the ports/FSM/layout shape that makes the gates meaningful.
- [docs/workflow.md](docs/workflow.md) — the spec-first methodology, routing rubric, tiers, and hard rules.
- [docs/toolchain.md](docs/toolchain.md) — every tool, one by one, with usefulness ratings.
- [docs/configuration.md](docs/configuration.md) — the `harness.toml` reference.

## Design principles (hard-won)

- **No silently-toothless gate.** If a project *looks* FSM-shaped (the source
  exists) but isn't configured, `dhx` **fails loudly** rather than skipping — the
  project's worst documented failure mode is a gate that passes while verifying
  nothing.
- **Pins are the single version authority.** The Docker image is built *from*
  `.harness/pins/*`, so in-container tool versions always match.
- **dhx verifies itself** with the universal subset of its own gates
  (fmt/clippy/test/file-size≤400/deny/machete) — an unverified verifier would be
  the ultimate toothless gate.

See [docs/](docs/) for the full manual and [docs/toolchain.md](docs/toolchain.md)
for per-tool detail.

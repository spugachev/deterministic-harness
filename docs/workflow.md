# Workflow — the methodology, the tiers, and the hard rules

## Not test-first; spec-first then route

This is **not** test-first TDD. You pin the *intent* in a spec, write the code,
then write the checks that prove it — and adequacy is enforced by mutation
testing + coverage, not by the order you typed things. For a new feature, work
the phases in this order (skip the ones the feature doesn't need — see the
routing rubric):

1. **REQ first.** Write `spec/requirements/REQ-NNN.md` with EARS acceptance
   criteria. Each criterion later needs a covering check — a BDD scenario, or a
   `(verified=kani|verus|tla|proptest|dst)` marker. `check-bdd-coverage` and
   `check-verified-markers` enforce this; the marker must be backed by a real
   link.
2. **TLA+ — only if the feature is concurrent / a protocol / has interleavings.**
   Model it in `spec/tla/*.tla` and run `dhx tlc`. For the lifecycle FSM you do
   **not** hand-write TLA+: it is generated from `state.rs::next` by `dhx regen`
   (edit the Rust → regen → commit). Every invariant you add needs an
   anti-vacuity entry in `spec/tla/mutations.toml` — `check-mutation-coverage`
   makes an invariant impossible to ship vacuous.
3. **BDD** for any externally observable behaviour: a Gherkin scenario in EARS
   form per acceptance criterion (`dhx check-bdd-style` / `check-bdd-coverage`).
4. **Implement** in `crates/core` (pure); put IO in an outer crate behind a port.
5. **Unit tests + coverage + mutants.** `dhx cov` holds the verified core at the
   `[coverage].core` bar (default 90% lines/functions); `dhx mutants` then proves
   those tests *kill* logic mutations — a test that passes with the logic
   inverted is weak and fails the gate.
6. **Kani** (bounded proof) for a pure, total, bounded function with an
   arithmetic/structural invariant; **Verus** for an unbounded ∀ / nonlinear
   postcondition.
7. **proptest** for any pure *law* — idempotence, monotonicity, round-trip,
   bounds. The broad, cheap workhorse.
8. **DST / Loom / TSAN / fuzz** as the feature demands (next section).

## The routing rubric — do NOT run every tool on everything

Ceremony is waste. Payoff tracks the feature's hardest question:

| The hardest question is… | Reach for |
|---|---|
| a pure law / invariant | **proptest**, then **Kani** if bounded-provable |
| arithmetic / overflow / boundary / dates | **clippy** (`arithmetic_side_effects`) + **Kani/Verus** |
| concurrency / interleavings / protocol | **TLA+** + **DST**; in-memory race → **Loom**; real-thread race → **TSAN** |
| HTTP-/externally-observable behaviour | **BDD** (+ **DST** for multi-step sequences) |
| raw / untrusted input parsing | **fuzz** |
| weak tests (adequacy) | **cargo-mutants** |
| spec ↔ code ↔ docs drift | the **meta-gates** (always on, ~free) |

## The three tiers — when each tool runs

There is **no CI**. These run locally and are the entire gate.

| Tier | Command | Cost | What runs |
|---|---|---|---|
| **Preflight** (every edit) | `dhx check` | ~seconds | fmt · regen · clippy · check-traceability · check-spec-sync · check-bdd-style · check-bdd-coverage · check-verified-markers · check-mutation-coverage · check-file-size · check-docs-counts — 11 cheap gates, **all failures aggregated in one pass** |
| **Quick** (pre-push) | `dhx verify --quick` | ~minutes | preflight + machete · gitleaks · deny · **test** · **cov** · check-kani-codegen · **kani** · **dst** (2k iters) |
| **Full** (pre-release) | `dhx verify --full` | ~tens of min | quick + outdated · geiger · **mutants** · **verus** · **miri** · **tsan** · **loom** · **dst** (4 fixed seeds + 1 random, 20k) · **fuzz** · **tlc** · **tlc --mutate** — **runs in the pinned Docker image** |

`dhx verify --full` on the host re-execs inside the project's Docker image (built
from `.harness/pins/*`), so every external tool matches the pins. No Docker
daemon ⇒ it fails loudly rather than silently falling back to host tools.

## Hard rules (non-negotiable — the gates enforce them)

- **clippy at 4 levels:** `all + pedantic + nursery + cargo` denied, plus a
  restriction allowlist (`unwrap_used`, `panic`, `indexing_slicing`,
  `arithmetic_side_effects`, `as_conversions`, cognitive-complexity ≤ 15, …).
  Run `dhx clippy` (it uses `--all-features`). The only escape is a site-level
  `#[allow(lint, reason = "…")]` that **must carry a reason**.
- **Determinism bans** (`clippy.toml`): no `SystemTime::now` / `Instant::now` /
  thread RNG in domain code — go through a port.
- **No `.rs` file over 400 lines** (`check-file-size`, no exemption — split it).
- **Coverage floor** on the verified core, proven non-vacuous by `dhx mutants`.
- **Every REQ criterion is covered** — a scenario, or a `(verified=…)` marker
  that is itself backed by a real `implements_in` link.
- **Every TLA+ invariant is non-vacuous** — a mutation in `mutations.toml`, or a
  justified exemption.
- **Conventional-commit prefixes** (`feat`/`fix`/`test`/`refactor`/…), scoped
  `(REQ-NNN)` for behaviour; the `commit-msg` hook enforces it, and
  `git log --grep=REQ-NNN` becomes the traceability matrix.
- **Hooks always on**; **`git --no-verify` is forbidden**.
- **`#![forbid(unsafe_code)]`** in shipped crates.

## Working with Claude Code

`dhx init` writes a `CLAUDE.md` (conventions + this methodology), `.claude/`
skills `/check` and `/verify`, and a `PostToolUse` hook that runs the cheap,
edit-stable subset (`cargo fmt --check` + `cargo check`) after every `.rs` edit.
Read the harness output before the diff — which proofs ran, which seeds tested,
the coverage delta, the mutation score. The gates catch what review misses.

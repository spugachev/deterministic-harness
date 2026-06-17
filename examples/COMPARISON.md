# Seat reservation — ON vs OFF, one service end to end

Two implementations of the same seat-reservation core, each built by a single
`claude -p` goal prompt:

- **`seats-off/`** — a plain `cargo` library. Prompt: implement the seat FSM,
  `reserve`, `parse_booking`, and hold-expiry; write happy-path smoke tests so
  `cargo test` passes; "a hurried first cut, ship it."
- **`seats-on/`** — a `dhx init` project. Prompt: same domain, but follow the
  harness workflow — a REQ per feature with EARS criteria, the FSM projected to
  TLA+ by `dhx regen`, typed errors, a property law per function, a Kani proof
  for the capacity invariant; reach a green `dhx check` then `dhx verify
  --quick`.

Same agent, same domain, good faith on both sides. This is the honest part: the
OFF arm here was **not** sloppy — it used `checked_add`, guarded the FSM, and
didn't panic. The difference the harness makes turned out to be about *defined
behaviour and verification*, not crash-vs-no-crash.

## What each arm produced

| | OFF | ON |
|---|---|---|
| Source files | 1 (`lib.rs`) | 5 domain modules + ports |
| Tests | 4 smoke | 20 (incl. 4 proptest laws) |
| Requirements / specs | 0 | 4 REQs (EARS) + TLA+ spec + `mutations.toml` |
| Machine-checked proofs | 0 | 1 Kani proof (`reserve_never_oversells`) |
| Error handling | values, lossy | typed `Result` (`OverError`, `ParseError`) |
| Gate run | `cargo test` (green) | `dhx verify --quick` **green (rc=0)** |

Every cheap gate and the quick tier ran on ON: regen-check, traceability,
spec-sync, bdd-coverage, **verified-markers**, **mutation-coverage**, file-size,
docs-counts, fmt, clippy (4 levels), machete, gitleaks, deny, test, llvm-cov,
kani-codegen, **kani**. (`dst` skipped cleanly — no simulation harness in this
core-only cut.)

## The bug classes, and how each mode did

The smoke tests passed in **both** arms, so none of the following is visible
from "it compiles and the tests pass." Each row is a distinct class of defect.

| Bug class | Where it lives | OFF behaviour | ON behaviour | Caught by (ON) |
|---|---|---|---|---|
| **Oversell (arithmetic boundary)** | `reserve(cap, held, qty)` | correct here — used `checked_add` (a careful OFF cut); but it is *only asserted by one smoke test*, never proved | `Result<u32, OverError>`, **proved** `held ≤ capacity` for all inputs | Kani `reserve_never_oversells` + a proptest law |
| **Silent data loss (parsing)** | `parse_booking("nodelim")` | returns `(0, "")` — malformed input is **silently swallowed**; the caller cannot tell a real qty 0 from a parse failure | returns `Err(ParseError)` — total and explicit | the `Result` type + a proptest "never panics on arbitrary input" law |
| **Silent illegal transition (FSM)** | `next(Confirmed, Hold)` | `(s, _) => s` — an illegal event is a **silent no-op**; nothing signals that a forbidden action was attempted | the transition table is the single source of truth, projected to TLA+ and model-checked; `ArchivedTerminal`-style invariants hold over all reachable states | TLA+ / TLC + `check-spec-sync` (code ↔ spec can't drift) |
| **Vacuous invariant (meta)** | the TLA+ spec | n/a — no spec | every invariant carries a known-violating mutation TLC must catch | `check-mutation-coverage` (1 mutated, 1 exempted) |
| **Unverifiable claim (meta)** | the requirements | n/a — no requirements | each `(verified=…)` marker is backed by a real proof/scenario link | `check-verified-markers` (2 backed claims) |
| **Adequacy / drift (meta)** | tests, docs, files | unmeasured | coverage floor on the core; counts and traceability kept honest; every file ≤ 400 lines | llvm-cov + docs-counts + traceability + file-size |

## Review

**The OFF arm is a fair, competent first cut — and that is the point.** It does
not crash. A reviewer skimming the three-line functions would approve it. Yet it
ships three behaviours a real caller would eventually be burned by: a parser
that turns garbage into a plausible-looking `(0, "")`, an FSM that accepts an
illegal action by quietly doing nothing, and an oversell guarantee that rests on
a single example. None of these is a panic; all three are the kind of *quiet
wrongness* that surfaces in production, not in `cargo test`. The OFF agent even
named two of them in its own closing note — which is exactly the trap: the gaps
are knowable, and still ship, because nothing in the loop forced the issue.

**The ON arm did not require a smarter agent — it changed what "done" means.**
Because a green `dhx check`/`verify --quick` was the bar, the same model was
pushed into typed `Result`s, a property law per function, a model-checked FSM,
and a Kani proof of the one invariant that matters (never oversell). The
parser-swallow and illegal-no-op classes simply cannot exist when the gate
demands a total `Result` and a spec the code is checked against. The cost was
real — four REQs, a TLA+ spec, ~20 tests, several `--write` regenerations of the
lock and counts — and that cost is the honest counterweight: on a genuinely
trivial function the harness would have been ceremony.

**Consistent with the earlier studies, payoff tracked bug-surface.** Seats is a
high-bug-surface domain — a capacity boundary, a state machine, a parser, a
time-driven expiry — so almost every tool had a real question to answer and the
ON arm's extra work bought genuine guarantees. The one tool that did *not* land
here was DST, because this cut has no multi-step/network harness yet; the
harness correctly *skipped* it rather than pretending, which is itself the
design working as intended.

**Bottom line.** With smoke tests alone, both arms look finished. The harness's
contribution is not catching a crash the OFF arm missed — it is converting three
classes of silent wrongness into either a compile-time type, a failing gate, or
a machine-checked proof, and leaving an auditable trail (REQs, spec, markers)
that the OFF arm has no equivalent of.

---

_Reproduce: `seats-off` → `cargo test`; `seats-on` → `dhx check` then `dhx
verify --quick` (needs the dhx toolchain on PATH)._

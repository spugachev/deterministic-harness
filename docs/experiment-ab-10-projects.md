# A/B experiment — 10 projects, harness ON vs OFF

**Question.** Does the dhx harness change *what ships*, not just how it feels?
Build the same five features twice — once **OFF** (plain `cargo` lib, "make it
compile + one smoke test, ship it") and once **ON** (a `dhx init` project, full
harness workflow: REQ → impl → tests + a proptest law → `dhx check` green). Both
arms were written by the same agent (`claude -p`), in good faith; the OFF prompt
asked for hurried-but-competent code, not sabotage.

**Method.** Five features chosen for sharp *bug-surface* (overflow, rounding,
date-sign, parsing, capacity). For each OFF arm, after it "looked done," its
latent bug was found *empirically* — by running a hostile input the smoke test
skipped, not by guessing. For each ON arm, the same hostile input was replayed
against the shipped implementation.

## Result

| # | Feature | OFF arm shipped | Smoke test | Empirical probe (the bug) | ON arm | Caught by |
|---|---------|-----------------|-----------|---------------------------|--------|-----------|
| 1 | `workload(&[u32]) -> u32` | `weights.iter().sum()` | ✅ pass | `[u32::MAX, u32::MAX]` → **`attempt to add with overflow` panic** | `fold(0, saturating_add)` | clippy `arithmetic_side_effects` + proptest |
| 2 | `blend(a,b,weight_a)` | `100 - wa` | ✅ pass | `weight_a = 200` → **`attempt to subtract with overflow` panic** | `MAX_WEIGHT.saturating_sub(w)`, clamped | clippy + proptest (result ∈ 0..=100 ∀ u8) |
| 3 | `due_in_days(now,due)` | `(due-now)/86400` | ✅ pass | overdue by 12 h → **returns `0` ("on time"), not `-1`** | `div_euclid` (true floor) | proptest (floor law) |
| 4 | `parse_line("<d>\|<text>")` | `line.chars().next()… / splitn` | ✅ pass | `"noseparator"` → **silently swallows the text** (and a multibyte lead byte risks a panic) | `-> Result<_, ParseError>`, total | proptest (never panics on arbitrary input) + the type |
| 5 | `RingBuf` fixed cap | correct (`len()` caps, wraps) | ✅ pass | len stays ≤ cap ✅ — **no bug** | same shape, + proptest invariant | — (honest negative) |

**Score: OFF shipped 4 real defects across 5 features** — two panics on plausible
input, one wrong-answer (sign/rounding), one silent data loss. **The smoke tests
caught 0 of 4** — every bug lived at a boundary, an extreme value, or an input
shape the happy path never touched. **ON shipped 0**, and every ON project
reached a green `dhx check` (9–11 tests each, REQ-002 written, FSM untouched).

## Reading

The harness did not make the agent smarter; it changed *what counts as done*.
OFF's bar was "compiles + the one example I thought of passes" — and at machine
speed that bar ships a bug per feature. ON's bar was "a stated property holds
over a thousand inputs, and the gate is green," which forced `saturating_add`,
`div_euclid`, clamping, and a `Result` return — the exact fixes.

Consistent with the earlier studies, **payoff tracked bug-surface, not effort**:
the four wins were all arithmetic / boundary / date / parsing — the formal
tools' sweet spot — and the fifth feature (a straightforward ring buffer) was a
fair tie, which is the honest expected result where the hardest question is not
a boundary. The lesson the pipeline encodes is *routing*: the cheap floor
(clippy + proptest) caught all four here at near-zero cost; the heavy
instruments are reserved for the features whose hardest question is theirs.

# Transfer-ledger service: harness ON vs OFF

Two independent headless `claude -p` agents built the **same** money-transfer
ledger from the **identical** domain spec (parse an untrusted `TRANSFER` command;
move integer-cents between accounts; no overdraft; **conservation — the sum of
balances is invariant**; idempotency by key; account lifecycle Open→Frozen→Closed;
safe under concurrent transfers). Only the process wrapper differs:

- **OFF** — `service-no-harness/` — bare `cargo`, "ship it fast" brief.
- **ON** — `service-with-harness/` — a `dhx init` scaffold, told to follow the
  spec-first workflow until `dhx check` + `dhx verify --quick` are green.

This domain was chosen to stress tools the earlier *seats* A/B left idle —
especially **fuzz** (an untrusted text parser) and **Kani/proptest** (a
conservation invariant that's easy to get subtly wrong). Everything below was
**verified independently** (re-run / re-probed, not taken from the agents'
self-reports).

## At a glance

| | OFF (no harness) | ON (harness) |
|---|---|---|
| Build wall-clock (`claude -p`) | **195 s** (~3 min) | **1020 s** (~17 min) |
| Rust files / LOC | 1 / **371** | 13 / ~1500 |
| Spec artifacts | 0 | 7 REQ + 7 BDD features (17 scenarios) + TLA+ + ADR + mutations |
| Tests | **1** smoke | **31** + proptest + **Kani** + **TLC** + **DST** + **fuzz target** |
| `apply_transfer` arithmetic | raw `+`/`-`, overflow **unguarded** | `checked_sub` + `checked_add` (both guarded) |
| Gate state | n/a | `dhx check` + `dhx verify --quick` **green** |

## The decisive finding — OFF shipped a real conservation bug; ON proved it away

Both agents handled the *obvious* overdraft check (`from < amount → reject`). The
difference is the **non-obvious** side: adding to the recipient.

**OFF — `to + amount` overflows; conservation broken.** The agent even *reasoned*
about it and wrote a comment:
> `// addition can't overflow because the total supply already fit in u64.`

That's a real invariant — **but the precondition is never enforced.**
`open_account(id, initial_cents)` accepts any `initial_cents` with no supply
bound, so two accounts opened near `u64::MAX` make the sum exceed `u64::MAX`, and
the next transfer's `to + amount` overflows: **debug → panic, release → silent
wrap that destroys money.** Verified by probe `supply_overflow_breaks_conservation`
→ `attempt to add with overflow`. A human reviewer would very likely accept that
confident comment — this is the dangerous kind of bug: *plausible and wrong.*

**ON — guarded by construction and PROVEN.** `apply_transfer(from, to, amount)`:
```rust
let new_from = from.checked_sub(amount)?;   // None ⇒ insufficient funds
let new_to   = to.checked_add(amount)?;     // None ⇒ would overflow the recipient
```
and a **tractable scalar Kani proof** verifies, exhaustively over *every* `u64`
triple in 0.5 s (no CBMC OOM):
```rust
#[kani::proof]
fn transfer_conserves_and_never_oversells() {
    let from: u64 = kani::any(); let to: u64 = kani::any(); let amount: u64 = kani::any();
    if let Some((nf, nt)) = apply_transfer(from, to, amount) {
        assert!(nf as u128 + nt as u128 == from as u128 + to as u128); // conservation
        assert!(nf <= from);                                           // no overdraft
    }
}
```
The proptest conservation law + DST concurrent-transfer test cover the
multi-account / interleaving behaviour Kani deliberately doesn't.

## Tool coverage this domain unlocked (vs seats)

| Tool | seats A/B | ledger A/B |
|---|---|---|
| **fuzz** | idle (no parser) | **exercised** — `parse_input` fuzz target on the untrusted `TRANSFER` line, panic-freedom |
| **Kani** | trivial bound proof | **decisive** — proves conservation + no-overflow over all `u64` triples |
| **proptest** | a law | conservation + no-overdraft + idempotency laws |
| **TLA+ / FSM** | lifecycle | account lifecycle Open→Frozen→Closed, 4 transitions, 2 anti-vacuity mutations |
| **DST** | skipped | concurrent multi-thread transfers conserve balance |

The fuzz angle also produced an honest *negative*: **OFF's parser is genuinely
panic-safe** (50k random-byte probe in the prior run, length-guarded indexing) —
so fuzz would find nothing there. The bug wasn't in parsing; it was in the
arithmetic, which is exactly where Kani/proptest bite.

## A gate caught a real spec↔code defect in ON too

Building the FSM, the harness's `dhx regen` FSM-extractor surfaced that an
**or-pattern in `next` silently dropped two `Close` transitions** — the generated
TLA+ would have model-checked an *incomplete* state machine. The fix (spell out
each `(state, event)` arm) is visible in the final `lifecycle.rs::next`. That is
the meta-gate catching drift the author didn't see — a class with no analogue in
the OFF arm, which has no spec to drift from.

## Cost, honestly

ON took ~17 min vs OFF's ~3. The warm gate loop itself is cheap (`dhx check` 1 s,
`verify --quick` 36 s warm); the bulk of the 17 min is authoring the spec + the
four-crate structure + the agent learning the workflow. OFF is a single 371-line
file. For a throwaway, OFF wins on speed. For a ledger — where a silent
money-destroying overflow is a real-money incident — ON's `checked_add` + Kani
proof is the difference between "looks right and a reviewer signed off" and
"proven correct over every input."

## Verdict

Unlike the seats A/B (where OFF's single mutex got the marquee invariant right
and the harness was overhead), **this harder domain put a real, review-passing
bug in the OFF arm** — an unguarded recipient overflow defended by a plausible
but false comment — and the harness eliminated it by construction (`checked_add`)
and *proved* the elimination (Kani over all `u64`, proptest, DST). It also
exercised the full toolchain the simpler domain left idle (fuzz, exhaustive Kani,
TLA+ FSM, DST) and caught an independent spec↔code drift via the FSM meta-gate.
Payoff still tracks bug-surface — and a conservation invariant over unbounded
integer arithmetic has a far larger bug-surface than an atomic counter.

---

### Reproduction

```sh
# OFF
cd service-no-harness && cargo test            # 1 test (the overflow is untested)
# ON (gates in the dhx image via the cached alias)
dhx() { docker run --rm -v "$PWD":/work -w /work \
          -v dhx-cargo-registry:/root/.cargo/registry \
          -v "dhx-target-$(basename "$PWD")":/work/target dhx:latest dhx "$@"; }
cd service-with-harness
cargo test --workspace      # 31 tests
dhx check                   # 11 meta-gates green (1 s warm)
dhx verify --quick          # + proptest, Kani (all u64 triples), TLC, DST (36 s warm)
```

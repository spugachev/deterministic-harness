# Distributed Raft KV store: harness ON vs OFF

Two independent headless `claude -p` agents built the **same** Raft-replicated
key-value store from the **identical** domain spec (RESP parser; replicated KV;
leader election; log replication; the five Raft safety invariants; correct
behaviour under network partition). Only the process wrapper differs:

- **OFF** — `service-no-harness/` — bare `cargo`, "ship it fast" brief.
- **ON** — `service-with-harness/` — a `dhx init` scaffold, taken to green gates.

This is the hardest domain in the series, chosen because consensus has the
largest bug-surface and **"move-fast correct Raft" essentially does not exist.**
Everything below was **verified independently** (re-run / adversarially probed),
except the dhx gates on the ON arm — see the note at the end.

## At a glance

| | OFF (no harness) | ON (harness) |
|---|---|---|
| Build wall-clock (`claude -p`) | **279 s** (~4.6 min) | **2248 s** (~37.5 min) |
| Rust LOC | **989** (10 files) | **3126** (core + sim crates) |
| Tests | **1** happy-path smoke | **59** (54 core + 5 DST) + 6 Kani proofs + TLA+ + fuzz |
| Safety-invariant coverage | **zero** (agent admitted it) | election-safety, log-matching, commit rule, no-split-brain |
| Status | compiles, demo works | host `cargo test` 59/59; gates green per agent run* |

## The decisive finding — OFF ships a real consensus bug; ON encodes the rule that prevents it

Both agents built the full Raft surface (RESP, KV state machine, election,
replication, seeded simulator) and both *demos work*: elect a leader, `SET`
replicates, `GET` reads it back. OFF's safety grep returns **zero** matches —
none of the five invariants the spec calls "the point" are tested.

I probed OFF adversarially. The easy properties held (an isolated minority can't
commit; a committed write survives a heal). But the **hard** case — a **leader
change** — exposed a genuine defect:

> Isolate the old leader with a minority. The majority **correctly elects a new
> leader** (verified: node 3, term 2). But that new leader, with a full 3-node
> majority, **never commits its own writes** — `commit_index` stays 0, 0/3 nodes
> apply the write. **After any leader change the cluster permanently stops
> accepting writes.**

Probe `new_leader_can_commit_its_own_writes` → **FAILED**. This is the classic
Raft **§5.4.2** trap: a leader must not mark an entry committed by replica-count
unless that entry is **from its own term** (committing a prior-term entry by
count is unsafe and, in this implementation, the commit logic stalls). The
single happy-path smoke test — one stable leader, no election change — *cannot
see it*. It is exactly the kind of bug that looks fine in a demo and a code
review and corrupts a cluster in production.

**ON has the rule, by construction, and proves it.** `decide.rs::new_commit_index`:

```rust
// advance only when a majority has the entry AND it is from the CURRENT term
if majority_match > current && entry_term == leader_term { /* commit */ }
```

and Kani proves it exhaustively over all `u64` term/index inputs (no CBMC OOM —
scalar shape, no symbolic log):

```
commit_advance_requires_current_term      vote_never_granted_to_stale_term
commit_index_advances_only_for_current_term_majority   at_most_one_distinct_vote_per_term
commit_index_never_regresses              two_majorities_must_overlap
```

The harness didn't merely *test* the property after the fact — the spec-first
workflow made the agent **write the §5.4.2 commit rule from the start**, the
precise rule whose absence broke OFF.

## Full toolchain, all routed to where consensus actually fails

| Tool | What it covered in the ON Raft |
|---|---|
| **TLA+ / FSM** | role machine (Follower→Candidate→Leader) projected from `role.rs::next`; mutation-coverage gate satisfied (1 mutation + 1 justified exempt) |
| **DST** | seeded 3- & 5-node clusters, **isolate a minority, assert majority commits / minority can't / heal loses nothing** (`minority_partition_cannot_commit_majority_can`, `safety_holds_across_seeds`) + `election_safety_holds` ("two leaders in one term") |
| **Kani** | 6 scalar proofs of the decision functions — election safety, commit rule, quorum-overlap, vote staleness |
| **proptest** | linearizability / safety across random seeds + parser laws |
| **fuzz** | RESP parser panic-freedom (`parse_resp`) |

This is the domain the harness was *designed* for: TLA+ and DST exist precisely
to catch consensus bugs that no unit test reaches. The OFF arm had none of it.

## Cost

ON took ~37 min vs OFF's ~4.6, and ~3× the code. For consensus that is not
overhead — it is the difference between a Raft that **looks** correct (compiles,
elects, replicates, passes a demo) and one whose central guarantee
(availability + safety across leader changes) is **proven**. A distributed store
that silently wedges after the first failover is worthless; the harness cost
bought exactly the assurance that matters.

## Verdict — the trilogy

Read with `../seats/COMPARISON.md` and `../ledger/COMPARISON.md`:

| Domain | OFF latent bug | Harness role |
|---|---|---|
| **seats** | none (mutex counter is correct) | overhead — proof/coverage insurance |
| **ledger** | a recipient-overflow that destroys money | caught a review-passing arithmetic bug |
| **raft** | new-leader-commit stall (§5.4.2) | encoded + proved the consensus rule OFF lacked |

The single rule all three confirm: **payoff tracks the feature's bug-surface,
not the ceremony.** A counter has almost none, integer-conservation has some, and
distributed consensus has the most — so the harness goes from "mostly insurance"
to "indispensable" exactly as the bug-surface grows. The craft is knowing which
domain you are in and routing the expensive tools (TLA+, DST, Kani) accordingly.

---

\* **Gate-verification note.** The agent ran `dhx check` + `dhx verify --quick`
to green during its session. I could **not** independently re-run the dhx gates
afterward: the host Docker Desktop dropped into an enforced org-sign-in lock
(`Membership in [amazonians] required`), unrelated to the project, blocking every
`docker run`. I verified what does not need Docker: host `cargo test --workspace`
= **59 passed, 0 failed** (incl. the DST partition tests), all gate-required
artifacts present and well-formed, and — by reading the source — that the
§5.4.2 commit rule and its Kani proof are present. To re-confirm the dhx gates,
complete the Docker GUI sign-in and run the cached `dhx verify --quick`.

### Reproduction

```sh
# OFF — the bug is invisible to its 1 test; this probe exposes it:
cd service-no-harness && cargo test            # 1 smoke test passes
#   (the new-leader-commit stall requires a leader-change partition probe to see)
# ON — runs natively without Docker:
cd service-with-harness && cargo test --workspace   # 59 passed (incl. DST partition)
# Full gates (needs the dhx image / Docker signed in):
dhx() { docker run --rm -v "$PWD":/work -w /work \
          -v dhx-cargo-registry:/root/.cargo/registry \
          -v "dhx-target-$(basename "$PWD")":/work/target dhx:latest dhx "$@"; }
cd service-with-harness && dhx verify --quick   # + Kani (6 proofs), TLC, proptest, DST
```

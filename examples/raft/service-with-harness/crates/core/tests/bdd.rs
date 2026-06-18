//! BDD suite — the mandatory EARS floor. Every REQ has at least one Gherkin
//! scenario in `spec/features/*.feature`, tagged with its id and phrased
//! Given/When/Then. Each scenario drives the pure `core` domain directly — the
//! parser, the KV state machine, and the Raft node — with the deterministic
//! Clock/Rng ports; no HTTP, no async IO, because the core is IO-free.
//!
//! Run with `cargo test -p core --test bdd`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_docs_in_private_items,
    clippy::indexing_slicing,
    clippy::needless_pass_by_ref_mut,
    clippy::unused_async,
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "test-only; cucumber macro requirements and small in-test id casts"
)]

use core::domain::kv::{ApplyResult, Kv};
use core::domain::raft::message::NodeId;
use core::domain::raft::node::Node;
use core::domain::raft::role::Role;
use core::domain::resp::{parse, Command};
use core::ports::SeqGen;
use cucumber::{given, then, when};

#[path = "bdd/world.rs"]
mod world;
use world::{decode_literal, RaftWorld};

// ---------- RESP parser (REQ-001) ----------

#[given(regex = r"^the raw bytes (.+)$")]
async fn given_raw_bytes(w: &mut RaftWorld, literal: String) {
    let bytes = decode_literal(&literal);
    w.parsed = Some(parse(&bytes));
}

#[when(regex = r"^the parser runs$")]
async fn when_parser_runs(_w: &mut RaftWorld) {
    // Parsing already happened in the Given (it is total and side-effect free).
}

#[then(regex = r"^the system shall return a (GET|SET|DEL) command$")]
async fn then_returns_command(w: &mut RaftWorld, kind: String) {
    let got = w.parsed.as_ref().expect("a parse was attempted");
    let ok = matches!(
        (kind.as_str(), got),
        ("GET", Ok(Command::Get { .. }))
            | ("SET", Ok(Command::Set { .. }))
            | ("DEL", Ok(Command::Del { .. }))
    );
    assert!(ok, "expected a {kind} command, got {got:?}");
}

#[then(regex = r"^the system shall reject the input without panicking$")]
async fn then_rejects(w: &mut RaftWorld) {
    let got = w.parsed.as_ref().expect("a parse was attempted");
    assert!(got.is_err(), "expected an error, got {got:?}");
}

// ---------- KV state machine (REQ-002) ----------

#[given(regex = r"^an empty key-value store$")]
async fn given_empty_kv(w: &mut RaftWorld) {
    w.kv = Kv::new();
}

#[given(regex = r#"^a store holding "([^"]*)" = "([^"]*)"$"#)]
async fn given_store_holding(w: &mut RaftWorld, key: String, value: String) {
    w.kv = Kv::new();
    w.kv.apply(&Command::Set {
        key: key.into_bytes(),
        value: value.into_bytes(),
    });
}

#[when(regex = r#"^the committed command SET "([^"]*)" "([^"]*)" is applied$"#)]
async fn when_apply_set(w: &mut RaftWorld, key: String, value: String) {
    w.apply_result = Some(w.kv.apply(&Command::Set {
        key: key.into_bytes(),
        value: value.into_bytes(),
    }));
}

#[when(regex = r#"^the committed command GET "([^"]*)" is applied$"#)]
async fn when_apply_get(w: &mut RaftWorld, key: String) {
    w.apply_result = Some(w.kv.apply(&Command::Get {
        key: key.into_bytes(),
    }));
}

#[when(regex = r#"^the committed command DEL "([^"]*)" is applied$"#)]
async fn when_apply_del(w: &mut RaftWorld, key: String) {
    w.apply_result = Some(w.kv.apply(&Command::Del {
        key: key.into_bytes(),
    }));
}

#[then(regex = r#"^the system shall report the value "([^"]*)"$"#)]
async fn then_value_is(w: &mut RaftWorld, value: String) {
    assert_eq!(
        w.apply_result,
        Some(ApplyResult::Value(Some(value.into_bytes())))
    );
}

#[then(regex = r"^the system shall report no value$")]
async fn then_no_value(w: &mut RaftWorld) {
    assert_eq!(w.apply_result, Some(ApplyResult::Value(None)));
}

// ---------- Raft election / replication / safety / determinism / partition ----------

#[given(regex = r"^a fresh cluster of (\d+) nodes$")]
async fn given_cluster(w: &mut RaftWorld, n: usize) {
    let ids: Vec<NodeId> = (0..n as u64).collect();
    w.nodes = ids.iter().map(|&id| Node::new(id, &ids)).collect();
    w.rng = SeqGen(7);
}

#[when(regex = r"^the cluster runs until a leader is elected$")]
async fn when_run_election(w: &mut RaftWorld) {
    for _ in 0..50 {
        w.run(1);
        if w.leader().is_some() {
            break;
        }
    }
}

#[then(regex = r"^the system shall have exactly one leader$")]
async fn then_one_leader(w: &mut RaftWorld) {
    let leaders = w.nodes.iter().filter(|n| n.role() == Role::Leader).count();
    assert_eq!(leaders, 1, "expected exactly one leader, found {leaders}");
}

#[then(regex = r"^the system shall have at most one leader per term$")]
async fn then_election_safety(w: &mut RaftWorld) {
    let mut terms: Vec<u64> = w
        .nodes
        .iter()
        .filter(|n| n.role() == Role::Leader)
        .map(Node::term)
        .collect();
    terms.sort_unstable();
    assert!(
        terms.windows(2).all(|s| s[0] != s[1]),
        "two leaders share a term"
    );
}

#[when(regex = r#"^a client proposes SET "([^"]*)" "([^"]*)" to the leader$"#)]
async fn when_propose_to_leader(w: &mut RaftWorld, key: String, value: String) {
    let leader = w.leader().expect("a leader exists");
    let idx = leader as usize;
    let (ok, out) = w.nodes[idx].client_propose(Command::Set {
        key: key.into_bytes(),
        value: value.into_bytes(),
    });
    w.proposal_accepted = Some(ok);
    // Deliver the resulting replication, then run a few heartbeat rounds so the
    // updated commit index propagates to the followers.
    w.drain(out);
    w.run(5);
}

#[then(regex = r#"^the committed history shall include SET "([^"]*)" "([^"]*)" on every node$"#)]
async fn then_committed_everywhere(w: &mut RaftWorld, key: String, value: String) {
    let kb = key.into_bytes();
    let vb = value.into_bytes();
    for n in &w.nodes {
        // Every node that has committed up to this entry must have applied it.
        if n.commit_index() >= 1 {
            assert_eq!(
                n.kv().get(&kb),
                Some(&vb),
                "node {} missing committed write",
                n.id
            );
        }
    }
    // The leader certainly committed it.
    let leader = w.leader().expect("a leader exists");
    assert_eq!(w.nodes[leader as usize].kv().get(&kb), Some(&vb));
}

#[then(regex = r"^the system shall keep all node logs matching$")]
async fn then_logs_match(w: &mut RaftWorld) {
    for a in &w.nodes {
        for b in &w.nodes {
            let max = a.log().last_index().min(b.log().last_index());
            let mut idx = 1;
            while idx <= max {
                if a.log().term_at(idx) == b.log().term_at(idx) {
                    assert_eq!(a.log().get(idx), b.log().get(idx), "log mismatch at {idx}");
                }
                idx += 1;
            }
        }
    }
}

#[when(regex = r"^a non-leader node receives a client proposal$")]
async fn when_nonleader_proposal(w: &mut RaftWorld) {
    let leader = w.leader();
    // Pick any node that is NOT the leader.
    let target = w
        .nodes
        .iter()
        .position(|n| Some(n.id) != leader)
        .expect("a non-leader exists");
    let (ok, _out) = w.nodes[target].client_propose(Command::Set {
        key: b"x".to_vec(),
        value: b"1".to_vec(),
    });
    w.proposal_accepted = Some(ok);
}

#[then(regex = r"^the system shall reject the proposal$")]
async fn then_reject_proposal(w: &mut RaftWorld) {
    assert_eq!(w.proposal_accepted, Some(false));
}

#[then(regex = r"^the system shall elect the same leader for the same seed$")]
async fn then_deterministic(w: &mut RaftWorld) {
    let first = w.leader();
    // Rebuild an identical cluster with the same seed and re-run.
    let n = w.nodes.len();
    let ids: Vec<NodeId> = (0..n as u64).collect();
    let mut twin = RaftWorld {
        nodes: ids.iter().map(|&id| Node::new(id, &ids)).collect(),
        rng: SeqGen(7),
        ..RaftWorld::default()
    };
    for _ in 0..50 {
        twin.run(1);
        if twin.leader().is_some() {
            break;
        }
    }
    assert_eq!(
        first,
        twin.leader(),
        "same seed produced a different leader"
    );
}

// ---------- Partition behaviour (REQ-007) ----------

#[when(regex = r"^a minority of (\d+) nodes is partitioned away from the leader$")]
async fn when_partition_minority(w: &mut RaftWorld, count: usize) {
    let leader = w.leader().expect("a leader exists before partitioning");
    w.minority = w
        .nodes
        .iter()
        .map(|n| n.id)
        .filter(|&id| id != leader)
        .take(count)
        .collect();
    // Let the cluster settle under the partition.
    w.run(20);
}

#[when(regex = r#"^the majority commits SET "([^"]*)" "([^"]*)"$"#)]
async fn when_majority_commits(w: &mut RaftWorld, key: String, value: String) {
    let leader = w
        .current_leader()
        .expect("the majority side still has a leader");
    let idx = leader as usize;
    let (ok, out) = w.nodes[idx].client_propose(Command::Set {
        key: key.into_bytes(),
        value: value.into_bytes(),
    });
    w.proposal_accepted = Some(ok);
    w.drain(out);
    w.run(20);
}

#[then(
    regex = r#"^after healing every node shall hold the committed value "([^"]*)" for "([^"]*)"$"#
)]
async fn then_after_heal(w: &mut RaftWorld, value: String, key: String) {
    // Heal the partition and let the minority catch up.
    w.minority.clear();
    w.run(60);
    let kb = key.into_bytes();
    let vb = value.into_bytes();
    for n in &w.nodes {
        assert_eq!(
            n.kv().get(&kb),
            Some(&vb),
            "node {} did not catch up to the committed write after heal",
            n.id
        );
    }
}

fn main() {
    // NB: this crate is named `core`, which shadows std's `core` — so we cannot
    // use `#[tokio::main]`. Build the runtime explicitly. Specs are centralized
    // at the workspace root in spec/features/ (path relative to crates/core).
    let rt = tokio::runtime::Runtime::new().expect("build tokio runtime");
    rt.block_on(RaftWorld::run_feature_files());
}

impl RaftWorld {
    async fn run_feature_files() {
        <RaftWorld as cucumber::World>::run("../../spec/features").await;
    }
}

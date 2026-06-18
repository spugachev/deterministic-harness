//! Deterministic simulation tests (DST) — the headline gate for the Raft core.
//!
//! Each test builds a seeded cluster, scripts elections / client writes / a
//! network partition / a heal, and asserts the protocol-level safety properties
//! that no single scalar proof can reach: at most one leader per term, a minority
//! partition cannot commit, the majority keeps serving, and after the heal NO
//! committed write is lost, duplicated, or reordered (linearizable committed
//! history). Everything is determined by the seed, so a failure replays exactly.
//!
//! Run with `cargo test -p sim --test dst`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "test-only: deterministic-seed casts and direct node indexing"
)]

use raftcore::domain::resp::Command;
use sim::{
    committed_history_consistent, committed_prefix, election_safety_holds, leaders,
    log_matching_holds, prefixes_consistent, Cluster, Partition,
};

/// Drive the cluster until a single leader emerges (or give up after a bound).
fn elect_leader(cluster: &mut Cluster) -> u64 {
    for _ in 0..200 {
        cluster.run_until_quiet(1);
        if let Some(id) = cluster.leader() {
            return id;
        }
    }
    panic!("no leader elected within the round budget");
}

fn set(k: u8, v: u8) -> Command {
    Command::Set {
        key: vec![k],
        value: vec![v],
    }
}

#[test]
fn a_single_leader_is_elected() {
    let mut c = Cluster::new(3, 0x00C0_FFEE);
    let leader = elect_leader(&mut c);
    assert!(c.ids().contains(&leader));
    assert!(election_safety_holds(&c.nodes), "two leaders in one term");
    assert_eq!(leaders(&c.nodes).len(), 1, "exactly one node leads");
}

#[test]
fn leader_replicates_and_commits_writes() {
    let mut c = Cluster::new(3, 0x1234_u64);
    let leader = elect_leader(&mut c);
    assert!(c.propose(leader, set(1, 11)));
    assert!(c.propose(leader, set(2, 22)));
    c.run_until_quiet(20);

    // The leader committed both writes and applied them.
    let leader_idx = usize::try_from(leader).unwrap();
    assert_eq!(c.nodes[leader_idx].commit_index(), 2);
    assert_eq!(c.nodes[leader_idx].kv().get(&[1]), Some(&vec![11]));
    assert_eq!(c.nodes[leader_idx].kv().get(&[2]), Some(&vec![22]));

    // Every node's committed history agrees (State Machine Safety).
    assert!(committed_history_consistent(&c.nodes));
    assert!(log_matching_holds(&c.nodes));
}

#[test]
fn minority_partition_cannot_commit_majority_can() {
    // 5 nodes; isolate a 2-node minority. The 3-node majority must keep serving;
    // the minority must not be able to commit anything.
    let mut c = Cluster::new(5, 0xABCD_u64);
    let leader = elect_leader(&mut c);

    // Choose a minority that EXCLUDES the current leader so the majority side
    // retains a leader and stays writable.
    let minority: Vec<u64> = c
        .ids()
        .iter()
        .copied()
        .filter(|&id| id != leader)
        .take(2)
        .collect();
    c.set_partition(Partition::isolate(&minority));

    // The majority side keeps committing.
    assert!(c.propose(leader, set(7, 70)));
    c.run_until_quiet(40);
    let leader_idx = usize::try_from(leader).unwrap();
    assert!(
        c.nodes[leader_idx].commit_index() >= 1,
        "majority must commit"
    );

    // A client hitting a minority node cannot get a write committed: either the
    // node is not a leader (rejects) or, if it spuriously believes it leads, it
    // can never reach a majority, so its commit index stays behind the write.
    let m = usize::try_from(minority[0]).unwrap();
    let before = c.nodes[m].commit_index();
    let _ = c.propose(minority[0], set(9, 99));
    c.run_until_quiet(40);
    let after = c.nodes[m].commit_index();
    // The minority node never commits the isolated write.
    assert_eq!(
        c.nodes[m].kv().get(&[9]),
        None,
        "minority must NOT commit a write"
    );
    assert!(after >= before);

    // No split-brain at the same term, and logs still match.
    assert!(election_safety_holds(&c.nodes));
    assert!(log_matching_holds(&c.nodes));
}

#[test]
fn heal_loses_no_committed_write_and_minority_catches_up() {
    let mut c = Cluster::new(5, 0x5EED_u64);
    let leader = elect_leader(&mut c);

    // Commit a write while fully connected.
    assert!(c.propose(leader, set(1, 1)));
    c.run_until_quiet(20);

    // Partition off a minority (excluding the leader) and commit more on the
    // majority side.
    let minority: Vec<u64> = c
        .ids()
        .iter()
        .copied()
        .filter(|&id| id != leader)
        .take(2)
        .collect();
    c.set_partition(Partition::isolate(&minority));
    assert!(c.propose(leader, set(2, 2)));
    assert!(c.propose(leader, set(3, 3)));
    c.run_until_quiet(40);

    let leader_idx = usize::try_from(leader).unwrap();
    let committed_before_heal = committed_prefix(&c.nodes[leader_idx]);
    assert!(
        committed_before_heal.len() >= 3,
        "majority kept serving during partition"
    );

    // Heal: the minority must catch up, and NO committed write is lost.
    c.heal();
    c.run_until_quiet(80);

    // Every node's committed prefix is consistent with the leader's — nothing
    // lost, duplicated, or reordered.
    for node in &c.nodes {
        let p = committed_prefix(node);
        assert!(
            prefixes_consistent(&p, &committed_before_heal),
            "node {} committed history diverged after heal",
            node.id
        );
    }
    // The minority caught up to at least what was committed before the heal.
    for &m in &minority {
        let mi = usize::try_from(m).unwrap();
        assert!(
            c.nodes[mi].commit_index() >= committed_before_heal.len() as u64,
            "minority node {m} did not catch up after heal"
        );
    }
    assert!(election_safety_holds(&c.nodes));
    assert!(log_matching_holds(&c.nodes));
    assert!(committed_history_consistent(&c.nodes));
}

proptest::proptest! {
    // Across many seeds, the core safety invariants hold for an arbitrary
    // election + a burst of writes: one leader per term, matching logs, and a
    // globally consistent committed history (linearizability of the commit log).
    #[test]
    fn safety_holds_across_seeds(seed in proptest::num::u64::ANY, writes in 0u8..8) {
        let mut c = Cluster::new(3, seed);
        // Elect (bounded — some adversarial seeds may not settle, that's fine).
        for _ in 0..200 {
            c.run_until_quiet(1);
            if c.leader().is_some() { break; }
        }
        if let Some(leader) = c.leader() {
            for k in 0..writes {
                c.propose(leader, set(k, k));
            }
            c.run_until_quiet(40);
        }
        // Invariants must hold regardless of whether an election settled.
        proptest::prop_assert!(election_safety_holds(&c.nodes));
        proptest::prop_assert!(log_matching_holds(&c.nodes));
        proptest::prop_assert!(committed_history_consistent(&c.nodes));
    }
}

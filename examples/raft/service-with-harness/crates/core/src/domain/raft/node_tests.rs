//! Unit tests for the [`Node`] driver: election, voting, and replication paths
//! exercised with the deterministic [`FixedClock`]/[`SeqGen`] ports. These pin
//! the concrete behaviour; the whole-cluster, interleaved, partitioned behaviour
//! is covered by the DST harness in the `sim` crate.

use super::*;
use crate::domain::raft::message::Message;
use crate::domain::resp::Command;
use crate::ports::{FixedClock, SeqGen};

fn set_cmd(k: u8) -> Command {
    Command::Set {
        key: vec![k],
        value: vec![k],
    }
}

#[test]
fn follower_holds_before_deadline_then_times_out() {
    let ids = [0, 1, 2];
    let mut n = Node::new(0, &ids);
    let mut rng = SeqGen(1);
    // Arm the election timer at t=0 by handling a heartbeat from a leader, which
    // pushes the deadline out to t = base + jitter (>= 10). Now t=1 is "early".
    n.handle(
        &FixedClock(0),
        &mut rng,
        1,
        &Message::AppendEntries {
            term: 1,
            leader_id: 1,
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![],
            leader_commit: 0,
        },
    );
    // Before the (re-armed) deadline: nothing.
    let early = n.tick(&FixedClock(1), &mut rng);
    assert!(early.is_empty());
    // Well past any jittered deadline: become candidate, broadcast RequestVote.
    let out = n.tick(&FixedClock(1000), &mut rng);
    assert_eq!(n.role(), Role::Candidate);
    assert_eq!(n.term(), 2); // term bumped from the heartbeat's term 1
    assert_eq!(out.len(), 2); // one per peer
    assert!(matches!(out[0].msg, Message::RequestVote { term: 2, .. }));
}

#[test]
fn candidate_with_majority_becomes_leader() {
    let ids = [0, 1, 2];
    let mut n = Node::new(0, &ids);
    let mut rng = SeqGen(1);
    n.tick(&FixedClock(1000), &mut rng); // → Candidate, term 1, self-vote
                                         // One peer grants → 2 of 3 = majority → Leader.
    let out = n.handle(
        &FixedClock(1000),
        &mut rng,
        1,
        &Message::RequestVoteReply {
            term: 1,
            granted: true,
        },
    );
    assert_eq!(n.role(), Role::Leader);
    // Becoming leader broadcasts initial AppendEntries (heartbeats).
    assert_eq!(out.len(), 2);
    assert!(matches!(out[0].msg, Message::AppendEntries { term: 1, .. }));
}

#[test]
fn split_vote_does_not_elect() {
    let ids = [0, 1, 2];
    let mut n = Node::new(0, &ids);
    let mut rng = SeqGen(1);
    n.tick(&FixedClock(1000), &mut rng); // → Candidate
    let out = n.handle(
        &FixedClock(1000),
        &mut rng,
        1,
        &Message::RequestVoteReply {
            term: 1,
            granted: false,
        },
    );
    assert_eq!(n.role(), Role::Candidate); // still only its own vote
    assert!(out.is_empty());
}

#[test]
fn higher_term_message_steps_leader_down() {
    let ids = [0, 1, 2];
    let mut n = Node::new(0, &ids);
    let mut rng = SeqGen(1);
    n.tick(&FixedClock(1000), &mut rng);
    n.handle(
        &FixedClock(1000),
        &mut rng,
        1,
        &Message::RequestVoteReply {
            term: 1,
            granted: true,
        },
    );
    assert_eq!(n.role(), Role::Leader);
    // A heartbeat from a higher term forces step-down.
    n.handle(
        &FixedClock(1000),
        &mut rng,
        2,
        &Message::AppendEntries {
            term: 5,
            leader_id: 2,
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![],
            leader_commit: 0,
        },
    );
    assert_eq!(n.role(), Role::Follower);
    assert_eq!(n.term(), 5);
}

#[test]
fn non_leader_rejects_client_proposals() {
    let ids = [0, 1, 2];
    let mut n = Node::new(0, &ids);
    let (accepted, out) = n.client_propose(set_cmd(1));
    assert!(!accepted);
    assert!(out.is_empty());
}

#[test]
fn leader_commits_after_majority_replicates() {
    let ids = [0, 1, 2];
    let mut leader = Node::new(0, &ids);
    let mut rng = SeqGen(1);
    // Win election.
    leader.tick(&FixedClock(1000), &mut rng);
    leader.handle(
        &FixedClock(1000),
        &mut rng,
        1,
        &Message::RequestVoteReply {
            term: 1,
            granted: true,
        },
    );
    assert_eq!(leader.role(), Role::Leader);
    // Propose a command — not yet committed (no follower acks).
    let (ok, out) = leader.client_propose(set_cmd(7));
    assert!(ok);
    assert_eq!(leader.commit_index(), 0);
    assert_eq!(out.len(), 2);
    // One follower acks the entry at index 1 → majority (leader + 1 of 3) → commit.
    leader.handle(
        &FixedClock(1000),
        &mut rng,
        1,
        &Message::AppendEntriesReply {
            term: 1,
            success: true,
            match_index: 1,
        },
    );
    assert_eq!(leader.commit_index(), 1);
    // The committed command is applied to the KV state machine.
    assert_eq!(leader.kv().get(&[7]), Some(&vec![7]));
}

#[test]
fn follower_accepts_and_applies_replicated_entries() {
    let ids = [0, 1, 2];
    let mut follower = Node::new(1, &ids);
    let mut rng = SeqGen(2);
    // Leader (id 0) at term 1 sends one entry with leader_commit=1.
    let entry = crate::domain::raft::log::Entry {
        term: 1,
        command: set_cmd(9),
    };
    let out = follower.handle(
        &FixedClock(0),
        &mut rng,
        0,
        &Message::AppendEntries {
            term: 1,
            leader_id: 0,
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![entry],
            leader_commit: 1,
        },
    );
    assert_eq!(follower.role(), Role::Follower);
    assert!(matches!(
        out[0].msg,
        Message::AppendEntriesReply {
            success: true,
            match_index: 1,
            ..
        }
    ));
    assert_eq!(follower.commit_index(), 1);
    assert_eq!(follower.kv().get(&[9]), Some(&vec![9]));
}

#[test]
fn follower_rejects_stale_leader() {
    let ids = [0, 1, 2];
    let mut follower = Node::new(1, &ids);
    let mut rng = SeqGen(2);
    // Bump follower's term to 3 via a higher-term vote request.
    follower.handle(
        &FixedClock(0),
        &mut rng,
        2,
        &Message::RequestVote {
            term: 3,
            candidate_id: 2,
            last_log_index: 0,
            last_log_term: 0,
        },
    );
    assert_eq!(follower.term(), 3);
    // A leader still at term 1 is stale → rejected.
    let out = follower.handle(
        &FixedClock(0),
        &mut rng,
        0,
        &Message::AppendEntries {
            term: 1,
            leader_id: 0,
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![],
            leader_commit: 0,
        },
    );
    assert!(matches!(
        out[0].msg,
        Message::AppendEntriesReply { success: false, .. }
    ));
}

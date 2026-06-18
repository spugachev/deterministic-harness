//! Cluster-wide safety predicates the DST harness asserts after every run.
//!
//! These read the nodes' logs/state and check the Raft safety invariants that
//! are statements about the *whole* cluster (not a single scalar step): Election
//! Safety, Log Matching, and the committed-prefix used for the linearizability
//! check. They are pure functions of the cluster snapshot.

use raftcore::domain::raft::message::NodeId;
use raftcore::domain::raft::node::Node;
use raftcore::domain::raft::role::Role;
use raftcore::domain::resp::Command;

/// **Election Safety**: at most one leader exists per term. Returns `true` iff
/// no two nodes are simultaneously Leader in the same term.
#[must_use]
pub fn election_safety_holds(nodes: &[Node]) -> bool {
    let mut leader_terms: Vec<u64> = nodes
        .iter()
        .filter(|n| n.role() == Role::Leader)
        .map(Node::term)
        .collect();
    leader_terms.sort_unstable();
    // A duplicate term among leaders would violate the invariant.
    leader_terms.windows(2).all(|w| w[0] != w[1])
}

/// **Log Matching**: for any two nodes, if they have an entry at the same index
/// with the same term, every preceding entry matches too. Returns `true` iff the
/// invariant holds across all node pairs.
#[must_use]
pub fn log_matching_holds(nodes: &[Node]) -> bool {
    for a in nodes {
        for b in nodes {
            if !pair_log_matches(a, b) {
                return false;
            }
        }
    }
    true
}

/// Check Log Matching for one ordered pair of nodes.
fn pair_log_matches(a: &Node, b: &Node) -> bool {
    let max = a.log().last_index().min(b.log().last_index());
    let mut idx = 1;
    while idx <= max {
        let (ta, tb) = (a.log().term_at(idx), b.log().term_at(idx));
        if ta == tb {
            // Same index & term ⇒ the actual entries (term+command) must match.
            if a.log().get(idx) != b.log().get(idx) {
                return false;
            }
        }
        idx = idx.saturating_add(1);
    }
    true
}

/// The committed command prefix observed at `node`: the commands applied to its
/// state machine, in log order, up to its commit index. Two nodes' committed
/// prefixes must be consistent (one a prefix of the other) — that is State
/// Machine Safety / linearizability of the committed history.
#[must_use]
pub fn committed_prefix(node: &Node) -> Vec<Command> {
    let mut out = Vec::new();
    let mut idx = 1;
    while idx <= node.commit_index() {
        if let Some(entry) = node.log().get(idx) {
            out.push(entry.command.clone());
        }
        idx = idx.saturating_add(1);
    }
    out
}

/// Are two committed prefixes consistent — is one a prefix of the other? A
/// `false` here means two nodes committed *different* commands at the same index
/// (a State Machine Safety violation / a lost-or-reordered write).
#[must_use]
pub fn prefixes_consistent(a: &[Command], b: &[Command]) -> bool {
    let common = a.len().min(b.len());
    a.iter().take(common).eq(b.iter().take(common))
}

/// Convenience: does the whole cluster's committed history agree? Every pair of
/// nodes must have consistent committed prefixes.
#[must_use]
pub fn committed_history_consistent(nodes: &[Node]) -> bool {
    let prefixes: Vec<Vec<Command>> = nodes.iter().map(committed_prefix).collect();
    for i in 0..prefixes.len() {
        for j in 0..prefixes.len() {
            if !prefixes_consistent(&prefixes[i], &prefixes[j]) {
                return false;
            }
        }
    }
    true
}

/// The set of node ids that currently believe they are Leader (any term) — a
/// helper for partition assertions.
#[must_use]
pub fn leaders(nodes: &[Node]) -> Vec<NodeId> {
    nodes
        .iter()
        .filter(|n| n.role() == Role::Leader)
        .map(|n| n.id)
        .collect()
}

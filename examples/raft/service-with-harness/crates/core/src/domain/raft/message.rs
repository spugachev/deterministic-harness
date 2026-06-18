//! Raft RPC messages exchanged between nodes, plus node identity.
//!
//! These are plain data — the node driver (`super::node`) consumes and produces
//! them, and the DST harness shuttles them over a simulated, partition-able bus.
//! Keeping them IO-free (no sockets, no serde requirement) is what lets the whole
//! protocol run deterministically in one process.

use super::log::Entry;

/// A node identifier within the fixed cluster (0-based).
pub type NodeId = u64;

/// An RPC sent from one node to another. Every message carries the sender's
/// `term`, which is how a node learns it is stale (and steps down).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    /// Candidate → peers: "vote for me in `term`".
    RequestVote {
        /// The candidate's term.
        term: u64,
        /// The candidate's id.
        candidate_id: NodeId,
        /// Index of the candidate's last log entry.
        last_log_index: u64,
        /// Term of the candidate's last log entry.
        last_log_term: u64,
    },
    /// Peer → candidate: the vote response.
    RequestVoteReply {
        /// The responder's term (so a stale candidate steps down).
        term: u64,
        /// Whether the vote was granted.
        granted: bool,
    },
    /// Leader → followers: replicate `entries` (empty = heartbeat).
    AppendEntries {
        /// The leader's term.
        term: u64,
        /// The leader's id.
        leader_id: NodeId,
        /// Index immediately preceding the new entries.
        prev_log_index: u64,
        /// Term of the `prev_log_index` entry.
        prev_log_term: u64,
        /// The entries to store (empty for a heartbeat).
        entries: Vec<Entry>,
        /// The leader's commit index.
        leader_commit: u64,
    },
    /// Follower → leader: the `AppendEntries` response.
    AppendEntriesReply {
        /// The responder's term.
        term: u64,
        /// Whether the entries were accepted (prefix matched).
        success: bool,
        /// On success, the follower's resulting last index (lets the leader
        /// advance `match_index` without re-deriving it).
        match_index: u64,
    },
}

impl Message {
    /// The term carried by this message — used by the receiver to detect a
    /// higher term and step down.
    #[must_use]
    pub fn term(&self) -> u64 {
        match self {
            Message::RequestVote { term, .. }
            | Message::RequestVoteReply { term, .. }
            | Message::AppendEntries { term, .. }
            | Message::AppendEntriesReply { term, .. } => *term,
        }
    }
}

/// An addressed message: who it is from, who it is to, and the payload. The DST
/// bus routes on `(from, to)` and may drop it under a partition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope {
    /// The sending node.
    pub from: NodeId,
    /// The destination node.
    pub to: NodeId,
    /// The RPC payload.
    pub msg: Message,
}

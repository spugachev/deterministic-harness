//! Core Raft data types: terms, node ids, log entries, and the RPC messages.

use crate::resp::Command;

pub type NodeId = u64;
pub type Term = u64;
/// 1-based log index (index 0 is the implicit empty prefix).
pub type LogIndex = u64;

/// One replicated log entry: a client command stamped with the term in which
/// the leader created it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub term: Term,
    pub command: Command,
}

/// The role a node currently believes itself to be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

/// Raft RPCs exchanged between nodes. Replies are modelled as their own
/// messages so the transport is a uniform message bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    RequestVote {
        term: Term,
        candidate: NodeId,
        last_log_index: LogIndex,
        last_log_term: Term,
    },
    RequestVoteReply {
        term: Term,
        voter: NodeId,
        granted: bool,
    },
    AppendEntries {
        term: Term,
        leader: NodeId,
        prev_log_index: LogIndex,
        prev_log_term: Term,
        entries: Vec<LogEntry>,
        leader_commit: LogIndex,
    },
    AppendEntriesReply {
        term: Term,
        from: NodeId,
        success: bool,
        /// On success, the index of the last entry the follower now has from
        /// this AppendEntries (so the leader can advance match/next).
        match_index: LogIndex,
    },
}

/// A message addressed to a destination node, as emitted by a node's step
/// function. The simulator is responsible for delivery (and may drop/delay it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub to: NodeId,
    pub msg: Message,
}

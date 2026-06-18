//! The Raft node: persistent + volatile state, election, and vote handling.
//!
//! A `Node` is driven entirely by events — an election/heartbeat `tick` (time
//! comes from a [`Clock`] port, jitter from an [`Rng`] port) and inbound
//! [`Message`]s. Every handler is a pure function of the node's current state
//! plus the event: it mutates `self` and *returns* the [`Envelope`]s to send,
//! never doing IO. That is what lets the DST harness run a whole cluster
//! deterministically and replay any failure from a seed.
//!
//! Role transitions go through the FSM in `super::role`; the safety arithmetic
//! (vote granting, commit advance, log matching) goes through `super::decide`;
//! log mechanics through `super::log`. The leader-side replication handlers live
//! in the `replicate` child module (which shares this module's private state).

use super::decide::{is_majority, should_grant_vote};
use super::log::Log;
use super::message::{Envelope, Message, NodeId};
use super::role::Role;
use crate::domain::kv::Kv;
use crate::ports::{Clock, Rng};
use std::collections::BTreeMap;

mod replicate;

/// The election timeout base (in clock ticks); the randomized timeout is this
/// plus a jitter in `[0, ELECTION_JITTER)`, so split votes resolve.
pub const ELECTION_TIMEOUT_BASE: i64 = 10;
/// The span of randomized election-timeout jitter (in ticks).
pub const ELECTION_JITTER: u64 = 10;
/// The leader's heartbeat interval (in ticks) — must be < the election base so
/// a healthy leader keeps followers from timing out.
pub const HEARTBEAT_INTERVAL: i64 = 3;

/// A single Raft node in a fixed cluster.
#[derive(Clone, Debug)]
pub struct Node {
    /// This node's id.
    pub id: NodeId,
    /// The fixed set of peer ids (excludes `id`).
    peers: Vec<NodeId>,
    /// Total cluster size (peers + self).
    cluster_size: u64,

    // --- Persistent state (would survive a crash in a real system) ---
    /// Latest term this node has seen.
    current_term: u64,
    /// Candidate voted for in the current term, if any.
    voted_for: Option<NodeId>,
    /// The replicated log.
    log: Log,

    // --- Volatile state ---
    /// Current role (Follower/Candidate/Leader).
    role: Role,
    /// Highest log index known to be committed.
    commit_index: u64,
    /// Highest log index applied to the state machine.
    last_applied: u64,
    /// The applied key-value state machine.
    kv: Kv,
    /// Deadline (clock time) at which the election timer fires.
    election_deadline: i64,
    /// Votes received this term (candidate only), by voter id.
    votes_received: BTreeMap<NodeId, bool>,

    // --- Leader volatile state (per peer) ---
    /// For each peer, the next log index to send.
    next_index: BTreeMap<NodeId, u64>,
    /// For each peer, the highest index known replicated.
    match_index: BTreeMap<NodeId, u64>,
    /// Next heartbeat deadline (leader only).
    heartbeat_deadline: i64,
}

impl Node {
    /// Create a fresh Follower node `id` in a cluster whose ids are `all_ids`.
    #[must_use]
    pub fn new(id: NodeId, all_ids: &[NodeId]) -> Self {
        let peers: Vec<NodeId> = all_ids.iter().copied().filter(|&p| p != id).collect();
        let cluster_size = all_ids.len() as u64;
        Self {
            id,
            peers,
            cluster_size,
            current_term: 0,
            voted_for: None,
            log: Log::new(),
            role: Role::Follower,
            commit_index: 0,
            last_applied: 0,
            kv: Kv::new(),
            election_deadline: 0,
            votes_received: BTreeMap::new(),
            next_index: BTreeMap::new(),
            match_index: BTreeMap::new(),
            heartbeat_deadline: 0,
        }
    }

    /// The node's current role.
    #[must_use]
    pub fn role(&self) -> Role {
        self.role
    }

    /// The node's current term.
    #[must_use]
    pub fn term(&self) -> u64 {
        self.current_term
    }

    /// The committed-and-applied key-value state machine.
    #[must_use]
    pub fn kv(&self) -> &Kv {
        &self.kv
    }

    /// The node's commit index.
    #[must_use]
    pub fn commit_index(&self) -> u64 {
        self.commit_index
    }

    /// A read-only view of the log (for the DST safety assertions).
    #[must_use]
    pub fn log(&self) -> &Log {
        &self.log
    }

    /// Reset the (randomized) election deadline from `now` using `rng` jitter.
    fn reset_election_deadline<C: Clock, R: Rng>(&mut self, clock: &C, rng: &mut R) {
        let jitter = rng.next_u64() % ELECTION_JITTER;
        let jitter = i64::try_from(jitter).unwrap_or(0);
        self.election_deadline = clock
            .now_unix()
            .saturating_add(ELECTION_TIMEOUT_BASE)
            .saturating_add(jitter);
    }

    /// Advance time: fire the election timeout (Follower/Candidate) or send
    /// heartbeats (Leader) if their deadlines have passed. Returns outgoing RPCs.
    pub fn tick<C: Clock, R: Rng>(&mut self, clock: &C, rng: &mut R) -> Vec<Envelope> {
        let now = clock.now_unix();
        match self.role {
            Role::Leader => {
                if now >= self.heartbeat_deadline {
                    self.heartbeat_deadline = now.saturating_add(HEARTBEAT_INTERVAL);
                    return self.broadcast_append_entries();
                }
                Vec::new()
            }
            Role::Follower | Role::Candidate => {
                if now >= self.election_deadline {
                    self.start_election(clock, rng)
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// Begin (or restart) an election: become Candidate, bump the term, vote for
    /// self, and request votes from every peer.
    fn start_election<C: Clock, R: Rng>(&mut self, clock: &C, rng: &mut R) -> Vec<Envelope> {
        self.current_term = self.current_term.saturating_add(1);
        self.role = Role::Candidate;
        self.voted_for = Some(self.id);
        self.votes_received.clear();
        self.votes_received.insert(self.id, true);
        self.reset_election_deadline(clock, rng);

        let msg = Message::RequestVote {
            term: self.current_term,
            candidate_id: self.id,
            last_log_index: self.log.last_index(),
            last_log_term: self.log.last_term(),
        };
        self.broadcast(&msg)
    }

    /// Build an addressed copy of `msg` for every peer.
    fn broadcast(&self, msg: &Message) -> Vec<Envelope> {
        self.peers
            .iter()
            .map(|&to| Envelope {
                from: self.id,
                to,
                msg: msg.clone(),
            })
            .collect()
    }

    /// Iterate over the peer ids (shared with the `replicate` child module).
    fn peers_iter(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.peers.iter().copied()
    }

    /// The `next_index` for peer `to`, defaulting to 1 if unset.
    fn next_index_for(&self, to: NodeId) -> u64 {
        self.next_index.get(&to).copied().unwrap_or(1)
    }

    /// Handle an inbound message, mutating state and returning replies/RPCs.
    pub fn handle<C: Clock, R: Rng>(
        &mut self,
        clock: &C,
        rng: &mut R,
        from: NodeId,
        msg: &Message,
    ) -> Vec<Envelope> {
        // Universal rule: any message with a higher term makes us a Follower and
        // adopts that term (Raft §5.1).
        if msg.term() > self.current_term {
            self.step_down(msg.term());
            self.reset_election_deadline(clock, rng);
        }
        match msg {
            Message::RequestVote { .. } => self.on_request_vote(clock, rng, from, msg),
            Message::RequestVoteReply { term, granted } => {
                self.on_vote_reply(*term, from, *granted)
            }
            Message::AppendEntries { .. } => self.on_append_entries(clock, rng, from, msg),
            Message::AppendEntriesReply {
                term,
                success,
                match_index,
            } => self.on_append_reply(from, *term, *success, *match_index),
        }
    }

    /// Revert to Follower at `term`, clearing per-term vote state.
    fn step_down(&mut self, term: u64) {
        self.current_term = term;
        self.role = Role::Follower;
        self.voted_for = None;
        self.votes_received.clear();
    }

    /// Handle a `RequestVote`, applying the [`should_grant_vote`] rule.
    fn on_request_vote<C: Clock, R: Rng>(
        &mut self,
        clock: &C,
        rng: &mut R,
        from: NodeId,
        msg: &Message,
    ) -> Vec<Envelope> {
        let Message::RequestVote {
            term,
            candidate_id,
            last_log_index,
            last_log_term,
        } = msg
        else {
            return Vec::new();
        };
        let already_voted = self.voted_for.is_some();
        let voted_for_this = self.voted_for == Some(*candidate_id);
        let grant = should_grant_vote(
            self.current_term,
            *term,
            already_voted,
            voted_for_this,
            self.log.last_term(),
            self.log.last_index(),
            *last_log_term,
            *last_log_index,
        );
        if grant {
            self.voted_for = Some(*candidate_id);
            self.reset_election_deadline(clock, rng);
        }
        vec![Envelope {
            from: self.id,
            to: from,
            msg: Message::RequestVoteReply {
                term: self.current_term,
                granted: grant,
            },
        }]
    }

    /// Handle a vote reply; on a majority, become Leader.
    fn on_vote_reply(&mut self, term: u64, from: NodeId, granted: bool) -> Vec<Envelope> {
        if self.role != Role::Candidate || term != self.current_term {
            return Vec::new();
        }
        self.votes_received.insert(from, granted);
        let yes = self.votes_received.values().filter(|&&g| g).count() as u64;
        if is_majority(yes, self.cluster_size) {
            self.become_leader();
            return self.broadcast_append_entries();
        }
        Vec::new()
    }

    /// Transition Candidate → Leader and initialise per-peer replication state.
    fn become_leader(&mut self) {
        self.role = Role::Leader;
        let next = self.log.last_index().saturating_add(1);
        self.next_index.clear();
        self.match_index.clear();
        for &p in &self.peers {
            self.next_index.insert(p, next);
            self.match_index.insert(p, 0);
        }
        self.heartbeat_deadline = 0; // fire a heartbeat on the next tick
    }
}

#[cfg(test)]
#[path = "node_tests.rs"]
mod node_tests;

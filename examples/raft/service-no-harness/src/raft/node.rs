//! A single Raft node: deterministic state + step functions.
//!
//! The node holds NO IO. It is driven by the simulator which feeds it ticks
//! and inbound messages and collects the outbound `Envelope`s it returns. All
//! time enters via `tick(now)`; all randomness for election-timeout jitter
//! enters via an injected `Rng`.

use std::collections::BTreeMap;

use crate::kv::{KvReply, KvState};
use crate::ports::Rng;
use crate::resp::Command;

use super::types::{Envelope, LogEntry, LogIndex, Message, NodeId, Role, Term};

/// Election timeout is drawn uniformly from `[BASE, BASE + JITTER)` ticks so
/// split votes resolve. Heartbeats go out every `HEARTBEAT` ticks.
const ELECTION_TIMEOUT_BASE: u64 = 10;
const ELECTION_TIMEOUT_JITTER: u64 = 10;
const HEARTBEAT_INTERVAL: u64 = 3;

pub struct RaftNode {
    pub id: NodeId,
    peers: Vec<NodeId>,
    pub role: Role,

    // --- Persistent state (in-memory here) ---
    pub current_term: Term,
    voted_for: Option<NodeId>,
    /// The replicated log. `log[i]` is entry at 1-based index `i + 1`.
    log: Vec<LogEntry>,

    // --- Volatile state ---
    pub commit_index: LogIndex,
    last_applied: LogIndex,
    pub kv: KvState,

    // --- Leader state ---
    next_index: BTreeMap<NodeId, LogIndex>,
    match_index: BTreeMap<NodeId, LogIndex>,

    // --- Candidate state ---
    votes_received: u64,

    // --- Timing ---
    now: u64,
    election_deadline: u64,
    last_heartbeat: u64,
}

impl RaftNode {
    pub fn new(id: NodeId, peers: Vec<NodeId>, rng: &mut dyn Rng) -> Self {
        let mut node = Self {
            id,
            peers,
            role: Role::Follower,
            current_term: 0,
            voted_for: None,
            log: Vec::new(),
            commit_index: 0,
            last_applied: 0,
            kv: KvState::new(),
            next_index: BTreeMap::new(),
            match_index: BTreeMap::new(),
            votes_received: 0,
            now: 0,
            election_deadline: 0,
            last_heartbeat: 0,
        };
        node.reset_election_deadline(rng);
        node
    }

    fn cluster_size(&self) -> usize {
        self.peers.len() + 1
    }

    fn majority(&self) -> usize {
        self.cluster_size() / 2 + 1
    }

    fn last_log_index(&self) -> LogIndex {
        self.log.len() as LogIndex
    }

    fn last_log_term(&self) -> Term {
        self.log.last().map_or(0, |e| e.term)
    }

    /// Term of the entry at 1-based `index`, or 0 for the empty prefix.
    fn term_at(&self, index: LogIndex) -> Term {
        if index == 0 {
            0
        } else {
            self.log
                .get((index - 1) as usize)
                .map_or(0, |e| e.term)
        }
    }

    fn reset_election_deadline(&mut self, rng: &mut dyn Rng) {
        let jitter = rng.gen_range(0, ELECTION_TIMEOUT_JITTER);
        self.election_deadline = self.now + ELECTION_TIMEOUT_BASE + jitter;
    }

    /// Step the logical clock. May trigger an election (Follower/Candidate) or
    /// a heartbeat (Leader). Returns messages to send.
    pub fn tick(&mut self, now: u64, rng: &mut dyn Rng) -> Vec<Envelope> {
        self.now = now;
        match self.role {
            Role::Leader => {
                if now >= self.last_heartbeat + HEARTBEAT_INTERVAL {
                    self.last_heartbeat = now;
                    self.broadcast_append_entries()
                } else {
                    Vec::new()
                }
            }
            Role::Follower | Role::Candidate => {
                if now >= self.election_deadline {
                    self.start_election(rng)
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// A Follower/Candidate whose timer fired becomes a Candidate for the next
    /// term and requests votes from all peers.
    fn start_election(&mut self, rng: &mut dyn Rng) -> Vec<Envelope> {
        self.role = Role::Candidate;
        self.current_term += 1;
        self.voted_for = Some(self.id);
        self.votes_received = 1; // votes for itself
        self.reset_election_deadline(rng);

        let msg = Message::RequestVote {
            term: self.current_term,
            candidate: self.id,
            last_log_index: self.last_log_index(),
            last_log_term: self.last_log_term(),
        };
        self.peers
            .iter()
            .map(|&to| Envelope {
                to,
                msg: msg.clone(),
            })
            .collect()
    }

    /// Handle an inbound message, returning replies/outbound messages.
    pub fn handle(&mut self, msg: Message, rng: &mut dyn Rng) -> Vec<Envelope> {
        // Any message carrying a higher term forces us to step down and adopt it.
        if let Some(term) = msg_term(&msg) {
            if term > self.current_term {
                self.current_term = term;
                self.role = Role::Follower;
                self.voted_for = None;
            }
        }

        match msg {
            Message::RequestVote {
                term,
                candidate,
                last_log_index,
                last_log_term,
            } => self.handle_request_vote(term, candidate, last_log_index, last_log_term, rng),
            Message::RequestVoteReply {
                term,
                voter: _,
                granted,
            } => self.handle_vote_reply(term, granted),
            Message::AppendEntries {
                term,
                leader,
                prev_log_index,
                prev_log_term,
                entries,
                leader_commit,
            } => self.handle_append_entries(
                term,
                leader,
                prev_log_index,
                prev_log_term,
                entries,
                leader_commit,
                rng,
            ),
            Message::AppendEntriesReply {
                term,
                from,
                success,
                match_index,
            } => self.handle_append_reply(term, from, success, match_index),
        }
    }

    fn handle_request_vote(
        &mut self,
        term: Term,
        candidate: NodeId,
        last_log_index: LogIndex,
        last_log_term: Term,
        rng: &mut dyn Rng,
    ) -> Vec<Envelope> {
        let mut granted = false;
        if term >= self.current_term {
            let can_vote = self.voted_for.is_none() || self.voted_for == Some(candidate);
            // Candidate's log must be at least as up-to-date as ours.
            let log_ok = (last_log_term, last_log_index)
                >= (self.last_log_term(), self.last_log_index());
            if can_vote && log_ok {
                granted = true;
                self.voted_for = Some(candidate);
                self.reset_election_deadline(rng);
            }
        }
        vec![Envelope {
            to: candidate,
            msg: Message::RequestVoteReply {
                term: self.current_term,
                voter: self.id,
                granted,
            },
        }]
    }

    fn handle_vote_reply(&mut self, term: Term, granted: bool) -> Vec<Envelope> {
        if self.role != Role::Candidate || term != self.current_term {
            return Vec::new();
        }
        if granted {
            self.votes_received += 1;
            if self.votes_received as usize >= self.majority() {
                return self.become_leader();
            }
        }
        Vec::new()
    }

    fn become_leader(&mut self) -> Vec<Envelope> {
        self.role = Role::Leader;
        let next = self.last_log_index() + 1;
        self.next_index.clear();
        self.match_index.clear();
        for &p in &self.peers {
            self.next_index.insert(p, next);
            self.match_index.insert(p, 0);
        }
        self.last_heartbeat = self.now;
        // Immediately assert leadership with an empty AppendEntries.
        self.broadcast_append_entries()
    }

    fn broadcast_append_entries(&self) -> Vec<Envelope> {
        self.peers
            .iter()
            .map(|&peer| self.append_entries_for(peer))
            .collect()
    }

    fn append_entries_for(&self, peer: NodeId) -> Envelope {
        let next = self.next_index.get(&peer).copied().unwrap_or(1);
        let prev_log_index = next - 1;
        let prev_log_term = self.term_at(prev_log_index);
        let entries: Vec<LogEntry> = self
            .log
            .iter()
            .skip(prev_log_index as usize)
            .cloned()
            .collect();
        Envelope {
            to: peer,
            msg: Message::AppendEntries {
                term: self.current_term,
                leader: self.id,
                prev_log_index,
                prev_log_term,
                entries,
                leader_commit: self.commit_index,
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_append_entries(
        &mut self,
        term: Term,
        leader: NodeId,
        prev_log_index: LogIndex,
        prev_log_term: Term,
        entries: Vec<LogEntry>,
        leader_commit: LogIndex,
        rng: &mut dyn Rng,
    ) -> Vec<Envelope> {
        // Stale leader: reject.
        if term < self.current_term {
            return vec![self.append_reply(leader, false, 0)];
        }
        // Valid current leader: (re)become follower and refresh our timer.
        self.role = Role::Follower;
        self.reset_election_deadline(rng);

        // Log-consistency check at prev_log_index.
        if prev_log_index > self.last_log_index()
            || self.term_at(prev_log_index) != prev_log_term
        {
            return vec![self.append_reply(leader, false, 0)];
        }

        // Append/overwrite: find the first conflict, then truncate and extend.
        let mut idx = prev_log_index; // 0-based offset into `log` for next entry
        for entry in entries {
            let pos = idx as usize;
            if pos < self.log.len() {
                if self.log[pos].term != entry.term {
                    // Conflict: drop this and everything after, then append.
                    self.log.truncate(pos);
                    self.log.push(entry);
                }
                // else: identical entry already present, skip.
            } else {
                self.log.push(entry);
            }
            idx += 1;
        }
        let match_index = idx;

        // Advance commit index to min(leaderCommit, last new entry).
        if leader_commit > self.commit_index {
            self.commit_index = leader_commit.min(self.last_log_index());
            self.apply_committed();
        }

        vec![self.append_reply(leader, true, match_index)]
    }

    fn append_reply(&self, leader: NodeId, success: bool, match_index: LogIndex) -> Envelope {
        Envelope {
            to: leader,
            msg: Message::AppendEntriesReply {
                term: self.current_term,
                from: self.id,
                success,
                match_index,
            },
        }
    }

    fn handle_append_reply(
        &mut self,
        term: Term,
        from: NodeId,
        success: bool,
        match_index: LogIndex,
    ) -> Vec<Envelope> {
        if self.role != Role::Leader || term != self.current_term {
            return Vec::new();
        }
        if success {
            self.match_index.insert(from, match_index);
            self.next_index.insert(from, match_index + 1);
            self.maybe_advance_commit();
        } else {
            // Step back and retry on the next heartbeat.
            let next = self.next_index.entry(from).or_insert(1);
            if *next > 1 {
                *next -= 1;
            }
        }
        Vec::new()
    }

    /// Leader commit rule: an entry from the current term replicated on a
    /// majority becomes committed.
    fn maybe_advance_commit(&mut self) {
        let n = self.last_log_index();
        let mut new_commit = self.commit_index;
        for index in (self.commit_index + 1)..=n {
            // Only commit entries from the current term (Raft §5.4.2).
            if self.term_at(index) != self.current_term {
                continue;
            }
            let mut count = 1; // leader has it
            for &p in &self.peers {
                if self.match_index.get(&p).copied().unwrap_or(0) >= index {
                    count += 1;
                }
            }
            if count >= self.majority() {
                new_commit = index;
            }
        }
        if new_commit > self.commit_index {
            self.commit_index = new_commit;
            self.apply_committed();
        }
    }

    /// Apply newly-committed entries to the KV state machine in log order.
    fn apply_committed(&mut self) {
        while self.last_applied < self.commit_index {
            self.last_applied += 1;
            let cmd = self.log[(self.last_applied - 1) as usize].command.clone();
            let _ = self.kv.apply(&cmd);
        }
    }

    // --- Client-facing leader API (used by the simulator) ---

    /// True if this node currently believes it is the leader.
    pub fn is_leader(&self) -> bool {
        self.role == Role::Leader
    }

    /// Client write: append to the leader's log. Returns the assigned index, or
    /// `None` if this node is not the leader. Replication happens on the next
    /// heartbeat / tick.
    pub fn client_append(&mut self, command: Command) -> Option<LogIndex> {
        if !self.is_leader() {
            return None;
        }
        self.log.push(LogEntry {
            term: self.current_term,
            command,
        });
        Some(self.last_log_index())
    }

    /// Read a key from the applied state machine.
    pub fn read(&self, command: &Command) -> KvReply {
        // GET is a pure read against committed/applied state.
        match command {
            Command::Get { key } => KvReply::Value(self.kv.get(key).cloned()),
            _ => KvReply::Ok,
        }
    }

    pub fn log_len(&self) -> usize {
        self.log.len()
    }

    pub fn entry_at(&self, index: LogIndex) -> Option<&LogEntry> {
        if index == 0 {
            None
        } else {
            self.log.get((index - 1) as usize)
        }
    }
}

/// The term carried by a message, if any (all four carry one).
fn msg_term(msg: &Message) -> Option<Term> {
    Some(match msg {
        Message::RequestVote { term, .. } => *term,
        Message::RequestVoteReply { term, .. } => *term,
        Message::AppendEntries { term, .. } => *term,
        Message::AppendEntriesReply { term, .. } => *term,
    })
}

//! Leader/follower log-replication handlers for [`Node`].
//!
//! Split out of `node.rs` to keep both files within the size budget; this module
//! shares `Node`'s private state (it is a child module). It holds the
//! `AppendEntries` machinery, the client-propose path, the commit-advance rule,
//! and the apply-to-state-machine loop — i.e. everything that turns a replicated
//! log into committed, applied KV state.

use super::super::decide::{is_majority, new_commit_index};
use super::super::log::Entry;
use super::super::message::{Envelope, Message, NodeId};
use super::super::role::Role;
use super::Node;
use crate::domain::kv::ApplyResult;
use crate::domain::resp::Command;
use crate::ports::{Clock, Rng};

impl Node {
    /// Leader: send each peer the entries it is missing (or a heartbeat).
    pub(super) fn broadcast_append_entries(&self) -> Vec<Envelope> {
        self.peers_iter()
            .map(|to| Envelope {
                from: self.id,
                to,
                msg: self.append_entries_for(to),
            })
            .collect()
    }

    /// Construct the `AppendEntries` payload tailored to peer `to` from its
    /// `next_index`.
    fn append_entries_for(&self, to: NodeId) -> Message {
        let next = self.next_index_for(to).max(1);
        let prev_log_index = next.saturating_sub(1);
        let prev_log_term = self.log.term_at(prev_log_index).unwrap_or(0);
        let entries = self.log.slice_from(next);
        Message::AppendEntries {
            term: self.current_term,
            leader_id: self.id,
            prev_log_index,
            prev_log_term,
            entries,
            leader_commit: self.commit_index,
        }
    }

    /// A client proposes a command. Only a Leader accepts it: it appends to its
    /// own log and replicates. Returns `(accepted, outgoing)`. A non-leader
    /// returns `(false, [])` — the client must retry against the leader. This is
    /// what makes a minority partition unable to commit.
    pub fn client_propose(&mut self, command: Command) -> (bool, Vec<Envelope>) {
        if self.role != Role::Leader {
            return (false, Vec::new());
        }
        self.log.append(Entry {
            term: self.current_term,
            command,
        });
        // A single-node cluster commits immediately; otherwise replicate.
        self.maybe_advance_commit();
        (true, self.broadcast_append_entries())
    }

    /// Handle an `AppendEntries` from a leader.
    pub(super) fn on_append_entries<C: Clock, R: Rng>(
        &mut self,
        clock: &C,
        rng: &mut R,
        from: NodeId,
        msg: &Message,
    ) -> Vec<Envelope> {
        let Message::AppendEntries {
            term,
            prev_log_index,
            prev_log_term,
            entries,
            leader_commit,
            ..
        } = msg
        else {
            return Vec::new();
        };
        // Reject anything from a stale leader.
        if *term < self.current_term {
            return self.append_reply(from, false, 0);
        }
        // Valid current-term leader contact: (re)become Follower, refresh timer.
        self.role = Role::Follower;
        self.reset_election_deadline(clock, rng);

        let ok = self
            .log
            .try_append_entries(*prev_log_index, *prev_log_term, entries);
        if !ok {
            return self.append_reply(from, false, 0);
        }
        // Advance commit index to min(leaderCommit, our last index) and apply.
        let new_last = self.log.last_index();
        if *leader_commit > self.commit_index {
            self.commit_index = (*leader_commit).min(new_last);
            self.apply_committed();
        }
        self.append_reply(from, true, new_last)
    }

    /// Build a one-element `AppendEntriesReply` envelope back to `to`.
    fn append_reply(&self, to: NodeId, success: bool, match_index: u64) -> Vec<Envelope> {
        vec![Envelope {
            from: self.id,
            to,
            msg: Message::AppendEntriesReply {
                term: self.current_term,
                success,
                match_index,
            },
        }]
    }

    /// Leader: handle an `AppendEntries` reply, advancing or backing off the peer.
    pub(super) fn on_append_reply(
        &mut self,
        from: NodeId,
        term: u64,
        success: bool,
        match_index: u64,
    ) -> Vec<Envelope> {
        if self.role != Role::Leader || term != self.current_term {
            return Vec::new();
        }
        if success {
            self.match_index.insert(from, match_index);
            self.next_index.insert(from, match_index.saturating_add(1));
            self.maybe_advance_commit();
        } else {
            // Back off next_index for this peer and retry on the next heartbeat.
            let cur = self.next_index_for(from);
            self.next_index.insert(from, cur.saturating_sub(1).max(1));
        }
        Vec::new()
    }

    /// Leader: advance `commit_index` to the highest index replicated on a
    /// majority *and* created in the current term (the [`new_commit_index`]
    /// rule), then apply newly-committed entries.
    pub(super) fn maybe_advance_commit(&mut self) {
        let last = self.log.last_index();
        let mut candidate = self.commit_index;
        let mut idx = self.commit_index.saturating_add(1);
        while idx <= last {
            let replicated = self.replication_count(idx);
            let entry_term = self.log.term_at(idx).unwrap_or(0);
            if is_majority(replicated, self.cluster_size) {
                candidate = new_commit_index(idx, candidate, self.current_term, entry_term);
            }
            idx = idx.saturating_add(1);
        }
        if candidate > self.commit_index {
            self.commit_index = candidate;
            self.apply_committed();
        }
    }

    /// How many nodes (including self) have `index` in their log.
    fn replication_count(&self, index: u64) -> u64 {
        let peers = self.match_index.values().filter(|&&m| m >= index).count() as u64;
        peers.saturating_add(1) // include the leader itself
    }

    /// Apply every committed-but-unapplied entry to the KV state machine, in log
    /// order. This is the only path that mutates `kv` — guaranteeing State
    /// Machine Safety: entries apply exactly once, in index order.
    fn apply_committed(&mut self) {
        let mut idx = self.last_applied.saturating_add(1);
        while idx <= self.commit_index {
            if let Some(entry) = self.log.get(idx) {
                let cmd = entry.command.clone();
                let _: ApplyResult = self.kv.apply(&cmd);
            }
            self.last_applied = idx;
            idx = idx.saturating_add(1);
        }
    }
}

//! The shared `RaftWorld` cucumber world and its driver helpers, split out of
//! `bdd.rs` to keep each file within the size budget. The step definitions in
//! `bdd.rs` operate on this state.

use core::domain::kv::{ApplyResult, Kv};
use core::domain::raft::message::{Envelope, NodeId};
use core::domain::raft::node::Node;
use core::domain::raft::role::Role;
use core::domain::resp::{Command, ParseError};
use core::ports::{FixedClock, SeqGen};

/// One world covers all of raftkv's behaviour: a parse result, a KV state
/// machine, and a small Raft cluster driven over the deterministic ports.
#[derive(cucumber::World, Debug)]
pub struct RaftWorld {
    /// Last RESP parse outcome.
    pub parsed: Option<Result<Command, ParseError>>,
    /// The standalone KV state machine (for the apply scenarios).
    pub kv: Kv,
    /// Last KV apply result.
    pub apply_result: Option<ApplyResult>,
    /// A cluster of nodes (for election/replication scenarios).
    pub nodes: Vec<Node>,
    /// Seeded RNG for the cluster.
    pub rng: SeqGen,
    /// Whether the last client proposal was accepted.
    pub proposal_accepted: Option<bool>,
    /// Node ids isolated as a minority (empty = fully connected).
    pub minority: Vec<NodeId>,
    /// Simulated time; advances each round so staggered (jittered) election
    /// deadlines actually elapse and an election can converge.
    pub time: i64,
}

impl Default for RaftWorld {
    fn default() -> Self {
        Self {
            parsed: None,
            kv: Kv::new(),
            apply_result: None,
            nodes: Vec::new(),
            rng: SeqGen(1),
            proposal_accepted: None,
            minority: Vec::new(),
            time: 0,
        }
    }
}

impl RaftWorld {
    /// A fully-connected step: every node ticks, then all queued messages are
    /// delivered, for `rounds` rounds. Enough to settle an election in a tiny
    /// in-test cluster.
    pub fn run(&mut self, rounds: usize) {
        for _ in 0..rounds {
            self.time = self.time.saturating_add(1);
            let mut bus: Vec<Envelope> = Vec::new();
            let clock = FixedClock(self.time);
            for i in 0..self.nodes.len() {
                let out = self.nodes[i].tick(&clock, &mut self.rng);
                bus.extend(out);
            }
            self.drain(bus);
        }
    }

    /// Deliver a starter `bus` of envelopes to quiescence at the current time
    /// (used right after a client proposal injects replication traffic).
    pub fn drain(&mut self, mut bus: Vec<Envelope>) {
        let clock = FixedClock(self.time);
        let mut budget = 400;
        while let Some(env) = bus.pop() {
            let to = env.to as usize;
            if to < self.nodes.len() && self.connected(env.from, env.to) {
                let more = self.nodes[to].handle(&clock, &mut self.rng, env.from, &env.msg);
                bus.extend(more);
            }
            budget -= 1;
            if budget == 0 {
                break;
            }
        }
    }

    /// Can a message from `from` reach `to` under the current partition?
    pub fn connected(&self, from: NodeId, to: NodeId) -> bool {
        if self.minority.is_empty() {
            return true;
        }
        self.minority.contains(&from) == self.minority.contains(&to)
    }

    /// The single leader at the highest term, if the election has settled.
    pub fn leader(&self) -> Option<NodeId> {
        let max_term = self.nodes.iter().map(Node::term).max().unwrap_or(0);
        let mut found = None;
        for n in &self.nodes {
            if n.role() == Role::Leader && n.term() == max_term {
                if found.is_some() {
                    return None; // more than one → not settled
                }
                found = Some(n.id);
            }
        }
        found
    }

    /// Any node that currently believes it is Leader. During a partition the
    /// isolated minority keeps bumping its term (so it is no longer the global
    /// max term), but it can never reach a majority to actually become Leader —
    /// so the single Leader role still marks the writable majority side.
    pub fn current_leader(&self) -> Option<NodeId> {
        let mut found = None;
        for n in &self.nodes {
            if n.role() == Role::Leader {
                if found.is_some() {
                    return None;
                }
                found = Some(n.id);
            }
        }
        found
    }
}

/// Decode a small test literal: `EMPTY` → no bytes; otherwise the UTF-8 of the
/// string with `\r`/`\n` escapes expanded.
pub fn decode_literal(s: &str) -> Vec<u8> {
    if s == "EMPTY" {
        return Vec::new();
    }
    s.replace("\\r", "\r").replace("\\n", "\n").into_bytes()
}

//! A deterministic simulation-testing (DST) harness for the Raft cluster.
//!
//! Everything runs in one process over a simulated message bus with the
//! [`raftcore::ports`] Clock/Rng, so a whole cluster run — elections, replication,
//! a network partition, and the heal — is fully determined by a `u64` seed and
//! the scripted operations. A failing run replays identically.
//!
//! The simulator deliberately lives OUTSIDE the verified core: it drives the
//! pure `raftcore::domain::raft::Node` and asserts the protocol-level safety
//! properties (one leader per term, no lost committed write, linearizable
//! committed history) that are statements about the *whole cluster*, which Kani
//! cannot reach but a seeded simulation can.

#![forbid(unsafe_code)]

use raftcore::domain::raft::message::{Envelope, NodeId};
use raftcore::domain::raft::node::Node;
use raftcore::domain::raft::role::Role;
use raftcore::domain::resp::Command;
use raftcore::ports::{Clock, SeqGen};
use std::cell::Cell;
use std::collections::{BTreeSet, VecDeque};

/// A clock the simulator advances by hand; reads are interior-mutable so a
/// `&Clock` can be handed to a node while the sim still owns the time.
#[derive(Debug, Default)]
pub struct SimClock {
    now: Cell<i64>,
}

impl SimClock {
    /// A clock starting at `t`.
    #[must_use]
    pub fn new(t: i64) -> Self {
        Self { now: Cell::new(t) }
    }
    /// Advance time by one tick.
    pub fn advance(&self, by: i64) {
        self.now.set(self.now.get().saturating_add(by));
    }
    /// The current time.
    #[must_use]
    pub fn get(&self) -> i64 {
        self.now.get()
    }
}

impl Clock for SimClock {
    fn now_unix(&self) -> i64 {
        self.now.get()
    }
}

/// A network partition: the cluster is split into two sides; a message crossing
/// the cut is dropped. An empty cut means a fully-connected network.
#[derive(Clone, Debug, Default)]
pub struct Partition {
    /// One side of the cut (the other side is "everyone else").
    side_a: BTreeSet<NodeId>,
}

impl Partition {
    /// No partition — every message is delivered.
    #[must_use]
    pub fn none() -> Self {
        Self {
            side_a: BTreeSet::new(),
        }
    }
    /// Isolate the nodes in `minority` from the rest of the cluster.
    #[must_use]
    pub fn isolate(minority: &[NodeId]) -> Self {
        Self {
            side_a: minority.iter().copied().collect(),
        }
    }
    /// Can a message from `from` reach `to` under this partition?
    #[must_use]
    pub fn connected(&self, from: NodeId, to: NodeId) -> bool {
        if self.side_a.is_empty() {
            return true;
        }
        self.side_a.contains(&from) == self.side_a.contains(&to)
    }
}

/// The simulated cluster: the nodes, the in-flight message queue, the clock, the
/// seeded RNG, and the current partition.
pub struct Cluster {
    /// The nodes, indexed by id (0-based, contiguous).
    pub nodes: Vec<Node>,
    clock: SimClock,
    rng: SeqGen,
    bus: VecDeque<Envelope>,
    partition: Partition,
    ids: Vec<NodeId>,
}

impl Cluster {
    /// Build a cluster of `n` nodes seeded with `seed`.
    #[must_use]
    pub fn new(n: u64, seed: u64) -> Self {
        let ids: Vec<NodeId> = (0..n).collect();
        let nodes = ids.iter().map(|&id| Node::new(id, &ids)).collect();
        Self {
            nodes,
            clock: SimClock::new(0),
            rng: SeqGen(seed),
            bus: VecDeque::new(),
            partition: Partition::none(),
            ids,
        }
    }

    /// Install a network partition (drops cross-cut messages from now on).
    pub fn set_partition(&mut self, p: Partition) {
        self.partition = p;
    }

    /// Heal any partition (full connectivity again).
    pub fn heal(&mut self) {
        self.partition = Partition::none();
    }

    /// The current simulated time.
    #[must_use]
    pub fn now(&self) -> i64 {
        self.clock.get()
    }

    /// Enqueue outgoing envelopes, dropping any that cross the partition cut.
    fn enqueue(&mut self, out: Vec<Envelope>) {
        for env in out {
            if self.partition.connected(env.from, env.to) {
                self.bus.push_back(env);
            }
            // else: dropped by the partition (as a real network would).
        }
    }

    /// Advance the clock by `by` ticks and let every node fire its timers.
    pub fn tick(&mut self, by: i64) {
        self.clock.advance(by);
        for i in 0..self.nodes.len() {
            let out = self.nodes[i].tick(&self.clock, &mut self.rng);
            self.enqueue(out);
        }
    }

    /// Deliver up to `max` queued messages (FIFO). Returns how many were
    /// delivered. Messages whose destination is now unreachable are dropped.
    pub fn deliver(&mut self, max: usize) -> usize {
        let mut delivered: usize = 0;
        for _ in 0..max {
            let Some(env) = self.bus.pop_front() else {
                break;
            };
            if !self.partition.connected(env.from, env.to) {
                continue; // partition formed after it was queued — drop it.
            }
            let Some(to) = usize::try_from(env.to)
                .ok()
                .filter(|&i| i < self.nodes.len())
            else {
                continue;
            };
            let out = self.nodes[to].handle(&self.clock, &mut self.rng, env.from, &env.msg);
            self.enqueue(out);
            delivered = delivered.saturating_add(1);
        }
        delivered
    }

    /// Run the cluster to quiescence: alternate ticking and delivering until no
    /// messages remain or `rounds` is exhausted. Deterministic for a given seed.
    pub fn run_until_quiet(&mut self, rounds: usize) {
        for _ in 0..rounds {
            self.tick(1);
            // Drain the bus this round (bounded so a heartbeat storm can't hang).
            let mut budget = 64;
            while !self.bus.is_empty() && budget > 0 {
                self.deliver(16);
                budget -= 1;
            }
        }
    }

    /// The id of a current leader, if exactly one node believes it leads at the
    /// highest term. Returns `None` if there is no settled leader.
    #[must_use]
    pub fn leader(&self) -> Option<NodeId> {
        let max_term = self.nodes.iter().map(Node::term).max().unwrap_or(0);
        let leaders: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.role() == Role::Leader && n.term() == max_term)
            .map(|n| n.id)
            .collect();
        match leaders.as_slice() {
            [only] => Some(*only),
            _ => None,
        }
    }

    /// Submit a client command to node `target`. Returns whether it was accepted
    /// (i.e. `target` is a leader). The resulting replication is enqueued.
    pub fn propose(&mut self, target: NodeId, cmd: Command) -> bool {
        let Some(idx) = usize::try_from(target)
            .ok()
            .filter(|&i| i < self.nodes.len())
        else {
            return false;
        };
        let (ok, out) = self.nodes[idx].client_propose(cmd);
        self.enqueue(out);
        ok
    }

    /// All node ids.
    #[must_use]
    pub fn ids(&self) -> &[NodeId] {
        &self.ids
    }
}

mod checks;
pub use checks::{
    committed_history_consistent, committed_prefix, election_safety_holds, leaders,
    log_matching_holds, prefixes_consistent,
};

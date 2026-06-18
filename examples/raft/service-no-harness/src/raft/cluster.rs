//! In-process cluster simulator: drives a set of `RaftNode`s over a logical
//! clock, routing messages through an in-memory bus. Fully deterministic given
//! a seed.

use std::collections::VecDeque;

use crate::ports::SplitMix64;
use crate::resp::Command;

use super::node::RaftNode;
use super::types::{Envelope, NodeId};

pub struct Cluster {
    pub nodes: Vec<RaftNode>,
    /// In-flight messages waiting to be delivered.
    bus: VecDeque<Envelope>,
    rng: SplitMix64,
    now: u64,
}

impl Cluster {
    /// Build a cluster of `size` nodes (ids 0..size) sharing the given seed.
    pub fn new(size: u64, seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed);
        let ids: Vec<NodeId> = (0..size).collect();
        let mut nodes = Vec::with_capacity(size as usize);
        for &id in &ids {
            let peers: Vec<NodeId> = ids.iter().copied().filter(|&p| p != id).collect();
            nodes.push(RaftNode::new(id, peers, &mut rng));
        }
        Self {
            nodes,
            bus: VecDeque::new(),
            rng,
            now: 0,
        }
    }

    fn node_mut(&mut self, id: NodeId) -> &mut RaftNode {
        &mut self.nodes[id as usize]
    }

    /// Advance the logical clock by one tick and deliver all currently-queued
    /// messages, then any messages those produce, until the bus drains.
    pub fn tick(&mut self) {
        self.now += 1;
        let now = self.now;

        // Fire timers.
        let mut pending: VecDeque<Envelope> = VecDeque::new();
        for i in 0..self.nodes.len() {
            let out = {
                let rng = &mut self.rng;
                self.nodes[i].tick(now, rng)
            };
            pending.extend(out);
        }
        self.bus.extend(pending);
        self.drain_bus();
    }

    /// Deliver queued messages until the bus is empty (settle the round).
    fn drain_bus(&mut self) {
        while let Some(env) = self.bus.pop_front() {
            let out = {
                let rng = &mut self.rng;
                self.nodes[env.to as usize].handle(env.msg, rng)
            };
            self.bus.extend(out);
        }
    }

    /// Run `n` ticks.
    pub fn run_ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.tick();
        }
    }

    /// Index of the current leader, if exactly one node believes it leads.
    pub fn leader(&self) -> Option<NodeId> {
        let leaders: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.is_leader())
            .map(|n| n.id)
            .collect();
        if leaders.len() == 1 {
            Some(leaders[0])
        } else {
            None
        }
    }

    /// Submit a client write to the current leader. Returns false if there is
    /// no leader to accept it.
    pub fn client_write(&mut self, command: Command) -> bool {
        match self.leader() {
            Some(id) => self.node_mut(id).client_append(command).is_some(),
            None => false,
        }
    }
}

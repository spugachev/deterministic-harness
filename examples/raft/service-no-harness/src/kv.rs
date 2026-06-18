//! KV state machine: applying GET/SET/DEL to a deterministic map.
//!
//! `apply` is a pure function of (current state, committed command). GET does
//! not mutate; it is modelled as a read returning the current value.

use std::collections::BTreeMap;

use crate::resp::Command;

/// The replicated key-value state. A `BTreeMap` keeps iteration deterministic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KvState {
    map: BTreeMap<String, String>,
}

/// The outcome of applying a command, mirroring RESP reply shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KvReply {
    /// GET hit / miss.
    Value(Option<String>),
    /// SET acknowledged.
    Ok,
    /// DEL: number of keys removed (0 or 1 here).
    Deleted(u64),
}

impl KvState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a committed command, returning the reply. Pure: the result is a
    /// function only of `self` and `cmd`.
    pub fn apply(&mut self, cmd: &Command) -> KvReply {
        match cmd {
            Command::Get { key } => KvReply::Value(self.map.get(key).cloned()),
            Command::Set { key, value } => {
                self.map.insert(key.clone(), value.clone());
                KvReply::Ok
            }
            Command::Del { key } => {
                let removed = self.map.remove(key).is_some();
                KvReply::Deleted(u64::from(removed))
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.map.get(key)
    }
}

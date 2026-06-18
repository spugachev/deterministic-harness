//! The replicated key-value state machine.
//!
//! Once Raft has *committed* a command (a majority has it, in log order), every
//! node feeds that command to this state machine via [`Kv::apply`]. Apply is a
//! **pure function of (current state, committed command)**: the same committed
//! log replayed on any node yields the same map — that determinism is what makes
//! State Machine Safety observable. There is no IO and no clock here; the map is
//! an ordered `BTreeMap` so equality and iteration are deterministic.

use super::resp::Command;
use std::collections::BTreeMap;

/// The deterministic key-value store: an ordered map of byte keys to byte
/// values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kv {
    map: BTreeMap<Vec<u8>, Vec<u8>>,
}

/// The result of applying a committed command to the [`Kv`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyResult {
    /// A `GET`: the value if present, else `None`.
    Value(Option<Vec<u8>>),
    /// A `SET`: always acknowledged.
    Ok,
    /// A `DEL`: `true` if a key was actually removed.
    Deleted(bool),
}

impl Kv {
    /// A fresh, empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    /// Apply one committed command, mutating the store and returning its result.
    /// Pure in the sense that the (state, command) pair fully determines both the
    /// new state and the result — no clock, no randomness, no IO.
    pub fn apply(&mut self, cmd: &Command) -> ApplyResult {
        match cmd {
            Command::Get { key } => ApplyResult::Value(self.map.get(key).cloned()),
            Command::Set { key, value } => {
                self.map.insert(key.clone(), value.clone());
                ApplyResult::Ok
            }
            Command::Del { key } => ApplyResult::Deleted(self.map.remove(key).is_some()),
        }
    }

    /// Read a key without mutating (a convenience for assertions / read paths).
    #[must_use]
    pub fn get(&self, key: &[u8]) -> Option<&Vec<u8>> {
        self.map.get(key)
    }

    /// Number of keys currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{ApplyResult, Kv};
    use crate::domain::resp::Command;

    fn set(k: &[u8], v: &[u8]) -> Command {
        Command::Set {
            key: k.to_vec(),
            value: v.to_vec(),
        }
    }
    fn get(k: &[u8]) -> Command {
        Command::Get { key: k.to_vec() }
    }
    fn del(k: &[u8]) -> Command {
        Command::Del { key: k.to_vec() }
    }

    #[test]
    fn get_missing_is_none() {
        let mut kv = Kv::new();
        assert_eq!(kv.apply(&get(b"x")), ApplyResult::Value(None));
    }

    #[test]
    fn set_then_get_returns_value() {
        let mut kv = Kv::new();
        assert_eq!(kv.apply(&set(b"x", b"1")), ApplyResult::Ok);
        assert_eq!(
            kv.apply(&get(b"x")),
            ApplyResult::Value(Some(b"1".to_vec()))
        );
    }

    #[test]
    fn set_overwrites() {
        let mut kv = Kv::new();
        kv.apply(&set(b"x", b"1"));
        kv.apply(&set(b"x", b"2"));
        assert_eq!(
            kv.apply(&get(b"x")),
            ApplyResult::Value(Some(b"2".to_vec()))
        );
    }

    #[test]
    fn del_reports_whether_present() {
        let mut kv = Kv::new();
        kv.apply(&set(b"x", b"1"));
        assert_eq!(kv.apply(&del(b"x")), ApplyResult::Deleted(true));
        assert_eq!(kv.apply(&del(b"x")), ApplyResult::Deleted(false));
        assert_eq!(kv.apply(&get(b"x")), ApplyResult::Value(None));
    }

    #[test]
    fn determinism_same_log_same_state() {
        // Two independent replays of the same committed log yield equal stores.
        let log = [set(b"a", b"1"), set(b"b", b"2"), del(b"a"), set(b"c", b"3")];
        let mut left = Kv::new();
        let mut right = Kv::new();
        for c in &log {
            left.apply(c);
        }
        for c in &log {
            right.apply(c);
        }
        assert_eq!(left, right);
        assert_eq!(left.len(), 2); // b, c
    }

    proptest::proptest! {
        // Replaying the same committed command sequence on two fresh stores
        // always yields identical state (State Machine Safety, locally).
        #[test]
        fn replay_is_deterministic(
            ops in proptest::collection::vec(0_u8..3, 0..40),
        ) {
            let cmds: Vec<Command> = ops.iter().enumerate().map(|(i, &op)| {
                let k = vec![u8::try_from(i % 4).unwrap_or(0)];
                let v = u8::try_from(i % 256).unwrap_or(0);
                match op {
                    0 => Command::Set { key: k, value: vec![v] },
                    1 => Command::Del { key: k },
                    _ => Command::Get { key: k },
                }
            }).collect();
            let mut a = Kv::new();
            let mut b = Kv::new();
            for c in &cmds { a.apply(c); }
            for c in &cmds { b.apply(c); }
            proptest::prop_assert_eq!(a, b);
        }

        // SET then GET on the same key always returns what was set (read-your-write
        // on the committed state machine).
        #[test]
        fn set_then_get_roundtrips(key in proptest::collection::vec(proptest::num::u8::ANY, 0..8),
                                   val in proptest::collection::vec(proptest::num::u8::ANY, 0..8)) {
            let mut kv = Kv::new();
            kv.apply(&Command::Set { key: key.clone(), value: val.clone() });
            proptest::prop_assert_eq!(kv.get(&key), Some(&val));
        }
    }
}

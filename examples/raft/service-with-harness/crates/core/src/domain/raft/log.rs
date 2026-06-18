//! The replicated Raft log: an append-only sequence of `(term, command)`
//! entries, 1-indexed as in the Raft paper (index 0 is the empty sentinel).
//!
//! The log is the shared object the safety invariants talk about: Log Matching,
//! Leader Completeness, and State Machine Safety are all statements about the
//! relationship between two nodes' logs. This type keeps the *mechanics* — append,
//! truncate-and-append on conflict, term-at-index lookup — pure and panic-free
//! (no indexing that can go out of bounds). The decision arithmetic lives in
//! `super::decide`.

use crate::domain::resp::Command;

/// A single log entry: the term in which the leader created it, plus the client
/// command it carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The term of the leader that created this entry.
    pub term: u64,
    /// The committed client command.
    pub command: Command,
}

/// An append-only Raft log. Entries are stored 0-based in `entries` but addressed
/// 1-based externally (`index 1` is `entries[0]`); index `0` denotes "before the
/// first entry".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    entries: Vec<Entry>,
}

impl Log {
    /// An empty log.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// The index of the last entry (0 if the log is empty).
    #[must_use]
    pub fn last_index(&self) -> u64 {
        self.entries.len() as u64
    }

    /// The term of the last entry (0 if the log is empty).
    #[must_use]
    pub fn last_term(&self) -> u64 {
        self.entries.last().map_or(0, |e| e.term)
    }

    /// The term of the entry at 1-based `index`, or `None` if out of range.
    /// `term_at(0)` is `Some(0)` (the sentinel before the first entry).
    #[must_use]
    pub fn term_at(&self, index: u64) -> Option<u64> {
        if index == 0 {
            return Some(0);
        }
        // 1-based → 0-based, via checked subtraction so it cannot underflow.
        let zero_based = index.checked_sub(1)?;
        let pos = usize::try_from(zero_based).ok()?;
        self.entries.get(pos).map(|e| e.term)
    }

    /// Does this log contain an entry at `index` whose term is `term`? This is
    /// the `AppendEntries` consistency check at the boundary.
    #[must_use]
    pub fn matches(&self, index: u64, term: u64) -> bool {
        self.term_at(index) == Some(term)
    }

    /// The entry at 1-based `index`, if present.
    #[must_use]
    pub fn get(&self, index: u64) -> Option<&Entry> {
        let zero_based = index.checked_sub(1)?;
        let pos = usize::try_from(zero_based).ok()?;
        self.entries.get(pos)
    }

    /// Append a single entry (leader path) and return its new index.
    pub fn append(&mut self, entry: Entry) -> u64 {
        self.entries.push(entry);
        self.last_index()
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over entries in `(low..=high)` (1-based, inclusive), clamped to
    /// what exists — used by the leader to build an `AppendEntries` payload.
    #[must_use]
    pub fn slice_from(&self, low: u64) -> Vec<Entry> {
        let start = low.saturating_sub(1);
        let Ok(start) = usize::try_from(start) else {
            return Vec::new();
        };
        self.entries.iter().skip(start).cloned().collect()
    }

    /// The follower-side `AppendEntries` body: given the leader's `prev_log_index`
    /// / `prev_log_term` and the `entries` that follow, splice them in. Returns
    /// `false` (rejecting) if the prefix does not match; otherwise truncates any
    /// conflicting suffix and appends, then returns `true`.
    ///
    /// This is where **Log Matching** is enforced mechanically: we only accept
    /// entries whose immediately-preceding entry matches, and a conflicting
    /// entry (same index, different term) is overwritten — never silently kept.
    pub fn try_append_entries(
        &mut self,
        prev_log_index: u64,
        prev_log_term: u64,
        entries: &[Entry],
    ) -> bool {
        // Consistency check: our entry at prev_log_index must have prev_log_term.
        if !self.matches(prev_log_index, prev_log_term) {
            return false;
        }
        // Walk the incoming entries; overwrite on the first term conflict, append
        // past the end. Index of the first new entry is prev_log_index + 1.
        for (offset, entry) in entries.iter().enumerate() {
            let idx = prev_log_index
                .saturating_add(offset as u64)
                .saturating_add(1);
            match self.term_at(idx) {
                Some(existing) if idx >= 1 && existing == entry.term => {
                    // Already have a matching entry here — skip (idempotent).
                }
                Some(_existing_conflict) if idx >= 1 => {
                    // Conflict: truncate from here and append the rest.
                    self.truncate_from(idx);
                    self.entries.push(entry.clone());
                }
                _ => {
                    // Past the end (or sentinel): append.
                    self.entries.push(entry.clone());
                }
            }
        }
        true
    }

    /// Drop every entry from 1-based `index` onward (inclusive).
    fn truncate_from(&mut self, index: u64) {
        let keep = index.saturating_sub(1);
        if let Ok(keep) = usize::try_from(keep) {
            self.entries.truncate(keep);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, Log};
    use crate::domain::resp::Command;

    fn entry(term: u64, n: u8) -> Entry {
        Entry {
            term,
            command: Command::Set {
                key: vec![n],
                value: vec![n],
            },
        }
    }

    #[test]
    fn empty_log_basics() {
        let log = Log::new();
        assert_eq!(log.last_index(), 0);
        assert_eq!(log.last_term(), 0);
        assert_eq!(log.term_at(0), Some(0));
        assert_eq!(log.term_at(1), None);
        assert!(log.matches(0, 0));
    }

    #[test]
    fn append_increments_index_and_term() {
        let mut log = Log::new();
        assert_eq!(log.append(entry(1, 1)), 1);
        assert_eq!(log.append(entry(1, 2)), 2);
        assert_eq!(log.last_index(), 2);
        assert_eq!(log.last_term(), 1);
        assert_eq!(log.term_at(1), Some(1));
        assert!(log.matches(2, 1));
        assert!(!log.matches(2, 2));
    }

    #[test]
    fn append_entries_rejects_on_prefix_mismatch() {
        let mut log = Log::new();
        log.append(entry(1, 1));
        // Claim prev at index 1 has term 2 (it has term 1) → reject.
        assert!(!log.try_append_entries(1, 2, &[entry(1, 2)]));
        assert_eq!(log.last_index(), 1);
    }

    #[test]
    fn append_entries_appends_past_end() {
        let mut log = Log::new();
        // prev=0/term0 (sentinel), append two fresh entries.
        assert!(log.try_append_entries(0, 0, &[entry(1, 1), entry(1, 2)]));
        assert_eq!(log.last_index(), 2);
    }

    #[test]
    fn append_entries_overwrites_conflict() {
        let mut log = Log::new();
        log.append(entry(1, 1));
        log.append(entry(1, 2)); // index 2, term 1
                                 // Leader sends a different term-2 entry at index 2.
        assert!(log.try_append_entries(1, 1, &[entry(2, 9)]));
        assert_eq!(log.last_index(), 2);
        assert_eq!(log.term_at(2), Some(2));
        assert_eq!(log.get(2).map(|e| e.term), Some(2));
    }

    #[test]
    fn append_entries_is_idempotent_on_match() {
        let mut log = Log::new();
        log.try_append_entries(0, 0, &[entry(1, 1), entry(1, 2)]);
        // Re-deliver the same batch: no growth, still accepted.
        assert!(log.try_append_entries(0, 0, &[entry(1, 1), entry(1, 2)]));
        assert_eq!(log.last_index(), 2);
    }
}

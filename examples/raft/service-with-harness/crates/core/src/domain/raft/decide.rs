//! The scalar Raft decision functions — the safety-critical arithmetic, factored
//! out of the stateful node so it can be proven EXHAUSTIVELY by Kani.
//!
//! Each function here is pure, total, panic-free, and takes a handful of scalar
//! `u64`/`bool` inputs (no `Vec`, no map, no loop). That is the tractable Kani
//! shape (see CLAUDE.md "Kani proof"): the safety laws of Raft are arithmetic
//! relations between term numbers, log indices, and a majority count, so we
//! prove THOSE here and leave the whole-cluster, multi-step behaviour to TLA+
//! and DST.
//!
//! All arithmetic is checked/saturating — no `+`/`-` that could overflow under
//! clippy's `arithmetic_side_effects` restriction.

/// Decide whether a node grants its vote to a candidate (the `RequestVote` rule).
///
/// Raft's **Election Safety** rests on this: a node votes for at most one
/// candidate per term, and only for a candidate whose log is at least as
/// up-to-date as its own. Encoded over scalars:
///
/// * `current_term` — the voter's term.
/// * `candidate_term` — the term in the `RequestVote` RPC.
/// * `already_voted` — has the voter already voted in `candidate_term`?
/// * `voted_for_this` — if it did, was it for THIS same candidate? (idempotent
///   re-request is allowed.)
/// * `last_log_term` / `last_log_index` — the voter's log head.
/// * `cand_last_log_term` / `cand_last_log_index` — the candidate's log head.
///
/// Returns `true` iff the vote is granted.
#[must_use]
#[allow(
    clippy::too_many_arguments,
    reason = "scalar decomposition of the RequestVote rule keeps it Kani-tractable; \
              bundling into a struct would hide the symbolic inputs from the proof"
)]
pub fn should_grant_vote(
    current_term: u64,
    candidate_term: u64,
    already_voted: bool,
    voted_for_this: bool,
    last_log_term: u64,
    last_log_index: u64,
    cand_last_log_term: u64,
    cand_last_log_index: u64,
) -> bool {
    // Never vote for a stale candidate (term behind ours).
    if candidate_term < current_term {
        return false;
    }
    // Within the candidate's term, we may vote at most once — unless this is the
    // very same candidate re-asking (idempotent).
    if already_voted && !voted_for_this {
        return false;
    }
    // The candidate's log must be at least as up-to-date as ours: higher last
    // term wins; on equal term the longer (>=) log wins.
    candidate_log_is_up_to_date(
        last_log_term,
        last_log_index,
        cand_last_log_term,
        cand_last_log_index,
    )
}

/// The Raft "up-to-date" log comparison used by [`should_grant_vote`]: a
/// candidate's log is at least as up-to-date as the voter's iff its last entry
/// has a higher term, or an equal term with an index `>=` the voter's.
#[must_use]
pub fn candidate_log_is_up_to_date(
    voter_last_term: u64,
    voter_last_index: u64,
    cand_last_term: u64,
    cand_last_index: u64,
) -> bool {
    if cand_last_term == voter_last_term {
        cand_last_index >= voter_last_index
    } else {
        cand_last_term > voter_last_term
    }
}

/// Compute the leader's new commit index (the `leaderCommit` advance rule).
///
/// **State Machine Safety / Leader Completeness** hinge on this: the leader may
/// only advance `commit_index` to an index `N` such that (a) a majority of nodes
/// have `N` in their log (`majority_match`), (b) `N > current` (commit only goes
/// forward — monotonic), and (c) the entry at `N` was created in the leader's
/// *current* term (`entry_term == leader_term`). Committing an entry from an
/// older term via majority alone is the classic Raft safety bug, so condition
/// (c) is mandatory.
///
/// Returns the new commit index — either `majority_match` (advance) or
/// `current` (hold). It NEVER moves backwards.
#[must_use]
pub fn new_commit_index(
    majority_match: u64,
    current: u64,
    leader_term: u64,
    entry_term: u64,
) -> u64 {
    if majority_match > current && entry_term == leader_term {
        majority_match
    } else {
        current
    }
}

/// Does a vote tally constitute a majority of the cluster?
///
/// `votes` granted out of a cluster of `cluster_size`. A majority is
/// `> cluster_size / 2`, which (with integer division) is exactly
/// `votes * 2 > cluster_size`. Using `*` on `u64` votes/size that are tiny
/// (cluster ≤ a handful) cannot overflow, but we saturate to stay panic-free for
/// any input. **No split-brain** follows from this being a strict majority: two
/// disjoint majorities of the same cluster are impossible.
#[must_use]
pub fn is_majority(votes: u64, cluster_size: u64) -> bool {
    votes.saturating_mul(2) > cluster_size
}

/// The **Log Matching** predicate on a single (index, term) coordinate pair.
///
/// Raft guarantees: if two logs contain an entry at the same index with the same
/// term, then the entries are identical and all preceding entries match. The
/// consistency check an `AppendEntries` performs at the boundary is exactly this:
/// the follower accepts iff its entry at `prev_log_index` has term
/// `prev_log_term`. This scalar predicate is that check.
///
/// Returns `true` iff the follower's entry at the probed index matches the
/// leader's expected (index, term).
#[must_use]
pub fn log_prefix_matches(
    follower_has_index: bool,
    follower_term_at_index: u64,
    leader_prev_term: u64,
) -> bool {
    follower_has_index && follower_term_at_index == leader_prev_term
}

#[cfg(test)]
mod tests {
    use super::{
        candidate_log_is_up_to_date, is_majority, log_prefix_matches, new_commit_index,
        should_grant_vote,
    };

    #[test]
    fn rejects_stale_candidate_term() {
        assert!(!should_grant_vote(5, 4, false, false, 0, 0, 0, 0));
    }

    #[test]
    fn grants_to_fresh_candidate_with_up_to_date_log() {
        // Same term, candidate index >= ours, not yet voted → grant.
        assert!(should_grant_vote(3, 3, false, false, 2, 7, 2, 7));
    }

    #[test]
    fn rejects_second_distinct_vote_in_term() {
        // Already voted this term, and not for this candidate → reject.
        assert!(!should_grant_vote(3, 3, true, false, 2, 7, 2, 7));
    }

    #[test]
    fn idempotent_revote_for_same_candidate() {
        // Already voted, but for THIS candidate → still grant (idempotent).
        assert!(should_grant_vote(3, 3, true, true, 2, 7, 2, 7));
    }

    #[test]
    fn rejects_candidate_with_shorter_log() {
        // Same last term, but candidate's log is shorter → not up-to-date.
        assert!(!should_grant_vote(3, 3, false, false, 5, 9, 5, 8));
    }

    #[test]
    fn up_to_date_prefers_higher_term_then_longer_index() {
        assert!(candidate_log_is_up_to_date(5, 100, 6, 0)); // higher term wins
        assert!(!candidate_log_is_up_to_date(6, 0, 5, 100)); // lower term loses
        assert!(candidate_log_is_up_to_date(5, 9, 5, 9)); // equal → >= ok
        assert!(!candidate_log_is_up_to_date(5, 9, 5, 8)); // equal term, shorter
    }

    #[test]
    fn commit_index_advances_only_for_current_term_majority() {
        assert_eq!(new_commit_index(7, 3, 4, 4), 7); // majority, current term → advance
        assert_eq!(new_commit_index(7, 3, 4, 2), 3); // old-term entry → hold
        assert_eq!(new_commit_index(2, 3, 4, 4), 3); // not ahead → hold
        assert_eq!(new_commit_index(3, 3, 4, 4), 3); // equal → hold (strictly >)
    }

    #[test]
    fn majority_is_strict() {
        assert!(!is_majority(1, 3)); // 1 of 3 is not a majority
        assert!(is_majority(2, 3)); // 2 of 3 is
        assert!(!is_majority(2, 5)); // 2 of 5 is not
        assert!(is_majority(3, 5)); // 3 of 5 is
        assert!(!is_majority(2, 4)); // 2 of 4 is not (need 3)
        assert!(is_majority(3, 4)); // 3 of 4 is
    }

    #[test]
    fn log_match_requires_present_and_equal_term() {
        assert!(log_prefix_matches(true, 4, 4));
        assert!(!log_prefix_matches(false, 4, 4)); // entry absent
        assert!(!log_prefix_matches(true, 3, 4)); // term mismatch
    }
}

// Kani proves the scalar safety laws EXHAUSTIVELY — every `u64` combination
// within the bounds, not sampled. These are the TRACTABLE shapes: a few scalar
// `kani::any()` inputs, pure arithmetic, no Vec/map/loop, so CBMC never blows up.
// The whole-cluster, multi-step, partition behaviour is proven by TLA+ and DST.
#[cfg(kani)]
mod proofs {
    use super::{candidate_log_is_up_to_date, is_majority, new_commit_index, should_grant_vote};

    /// Election Safety, scalar form: a node never grants a vote to a candidate
    /// whose term is behind its own — the precondition that lets at most one
    /// leader win a term.
    #[kani::proof]
    fn vote_never_granted_to_stale_term() {
        let current_term: u64 = kani::any();
        let candidate_term: u64 = kani::any();
        let already_voted: bool = kani::any();
        let voted_for_this: bool = kani::any();
        let llt: u64 = kani::any();
        let lli: u64 = kani::any();
        let clt: u64 = kani::any();
        let cli: u64 = kani::any();
        kani::assume(candidate_term < current_term);
        assert!(!should_grant_vote(
            current_term,
            candidate_term,
            already_voted,
            voted_for_this,
            llt,
            lli,
            clt,
            cli,
        ));
    }

    /// A node that has already voted for a DIFFERENT candidate this term never
    /// grants a second, distinct vote — one vote per term (Election Safety).
    #[kani::proof]
    fn at_most_one_distinct_vote_per_term() {
        let current_term: u64 = kani::any();
        let candidate_term: u64 = kani::any();
        let llt: u64 = kani::any();
        let lli: u64 = kani::any();
        let clt: u64 = kani::any();
        let cli: u64 = kani::any();
        // Already voted for someone else this (>= current) term.
        assert!(!should_grant_vote(
            current_term,
            candidate_term,
            true,  // already_voted
            false, // not for this candidate
            llt,
            lli,
            clt,
            cli,
        ));
    }

    /// State Machine Safety, scalar form: the commit index is MONOTONIC — the
    /// advance rule never moves it backwards, for any inputs.
    #[kani::proof]
    fn commit_index_never_regresses() {
        let majority_match: u64 = kani::any();
        let current: u64 = kani::any();
        let leader_term: u64 = kani::any();
        let entry_term: u64 = kani::any();
        let next = new_commit_index(majority_match, current, leader_term, entry_term);
        assert!(next >= current);
    }

    /// The commit index only advances on an entry from the leader's CURRENT term
    /// — the rule that prevents committing a stale-term entry by majority alone
    /// (the classic Raft safety bug). If it advanced, the entry term equalled the
    /// leader term.
    #[kani::proof]
    fn commit_advance_requires_current_term() {
        let majority_match: u64 = kani::any();
        let current: u64 = kani::any();
        let leader_term: u64 = kani::any();
        let entry_term: u64 = kani::any();
        let next = new_commit_index(majority_match, current, leader_term, entry_term);
        if next > current {
            assert!(entry_term == leader_term);
            assert!(next == majority_match);
        }
    }

    /// No split-brain, scalar form: two disjoint vote sets cannot both be a
    /// majority of the same cluster. If `a` and `b` are each a strict majority of
    /// `n`, then `a + b > n`, so they must overlap (share at least one voter) —
    /// hence at most one candidate wins a term.
    #[kani::proof]
    fn two_majorities_must_overlap() {
        let a: u64 = kani::any();
        let b: u64 = kani::any();
        let n: u64 = kani::any();
        // Bound the inputs so the multiplication in `is_majority` stays exact and
        // the proof stays tiny (the law is identical for all n).
        kani::assume(n <= 16);
        kani::assume(a <= n);
        kani::assume(b <= n);
        if is_majority(a, n) && is_majority(b, n) {
            // a*2 > n and b*2 > n  ⇒  (a+b)*2 > 2n  ⇒  a+b > n  ⇒  overlap.
            assert!(a.saturating_add(b) > n);
        }
    }

    /// The up-to-date comparison is a total order tie-break: it is reflexive
    /// (a log is always at least as up-to-date as itself).
    #[kani::proof]
    fn up_to_date_is_reflexive() {
        let t: u64 = kani::any();
        let i: u64 = kani::any();
        assert!(candidate_log_is_up_to_date(t, i, t, i));
    }
}

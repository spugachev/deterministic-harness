//! The Raft node role state machine: Follower to Candidate to Leader to Follower.
//!
//! This is the pure transition function wired into `harness.toml` `[fsm]`; dhx
//! regen generates the TLA+ spec from it. The role transitions are the control
//! skeleton of the protocol -- term numbers, logs, and vote counting live in the
//! scalar decision functions and the node driver; this enum captures only which
//! role a node currently plays and the events that move it.
//!
//! A returned None means the event does not change the role / is not a legal
//! role transition in that state. Some(role) is the new role.

/// Which role a node is currently playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// Passive: replicates the leader's log and votes; becomes a Candidate on
    /// election timeout.
    Follower,
    /// Standing for election: has incremented its term and requested votes.
    Candidate,
    /// Won a majority of votes for its term; the sole writer.
    Leader,
}

/// The role-level events that drive a transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Event {
    /// The election timer fired without hearing from a current leader.
    ElectionTimeout,
    /// This Candidate collected votes from a majority of the cluster.
    MajorityVotes,
    /// Observed a term strictly greater than this node's current term, from any
    /// RPC -- the node must revert to Follower and adopt the higher term.
    HigherTerm,
    /// Received a valid `AppendEntries` (heartbeat) from a legitimate current
    /// leader, so a Candidate steps down to Follower.
    LeaderAppendEntries,
}

/// The role transition function: pure, total, and panic-free.
///
/// Returns `Some(new_role)` for a legal transition and `None` when the event
/// leaves the role unchanged or is not a legal role move in that state. This is
/// the function dhx regen lifts into TLA+.
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "each (state, event) arm is listed separately and literally so the \
              dhx regen FSM extractor can lift one TLA+ transition per arm; merging \
              arms with or-patterns would collapse distinct transitions"
)]
pub fn next(state: Role, event: Event) -> Option<Role> {
    match (state, event) {
        (Role::Follower, Event::ElectionTimeout) => Some(Role::Candidate),
        (Role::Candidate, Event::ElectionTimeout) => Some(Role::Candidate),
        (Role::Candidate, Event::MajorityVotes) => Some(Role::Leader),
        (Role::Candidate, Event::LeaderAppendEntries) => Some(Role::Follower),
        (Role::Candidate, Event::HigherTerm) => Some(Role::Follower),
        (Role::Leader, Event::HigherTerm) => Some(Role::Follower),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{next, Event, Role};

    #[test]
    fn follower_times_out_into_candidate() {
        assert_eq!(
            next(Role::Follower, Event::ElectionTimeout),
            Some(Role::Candidate)
        );
    }

    #[test]
    fn candidate_wins_majority_becomes_leader() {
        assert_eq!(
            next(Role::Candidate, Event::MajorityVotes),
            Some(Role::Leader)
        );
    }

    #[test]
    fn candidate_restarts_election_on_timeout() {
        assert_eq!(
            next(Role::Candidate, Event::ElectionTimeout),
            Some(Role::Candidate)
        );
    }

    #[test]
    fn candidate_steps_down_on_leader_heartbeat() {
        assert_eq!(
            next(Role::Candidate, Event::LeaderAppendEntries),
            Some(Role::Follower)
        );
    }

    #[test]
    fn higher_term_demotes_candidate_and_leader() {
        assert_eq!(
            next(Role::Candidate, Event::HigherTerm),
            Some(Role::Follower)
        );
        assert_eq!(next(Role::Leader, Event::HigherTerm), Some(Role::Follower));
    }

    #[test]
    fn illegal_role_moves_return_none() {
        assert_eq!(next(Role::Follower, Event::MajorityVotes), None);
        assert_eq!(next(Role::Leader, Event::ElectionTimeout), None);
        assert_eq!(next(Role::Follower, Event::LeaderAppendEntries), None);
    }
}

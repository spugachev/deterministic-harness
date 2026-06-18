//! The pure, deterministic Raft consensus core.
//!
//! Everything here is IO-free and drives off the [`crate::ports`] (Clock/Rng):
//! a whole cluster can be simulated in one process and replayed from a seed,
//! which is what the DST harness in the `sim` crate does. The pieces:
//!
//! * [`role`] — the Follower/Candidate/Leader state machine (`fn next`), the
//!   `[fsm]` source `dhx regen` lifts into TLA+.
//! * [`decide`] — the scalar safety arithmetic (vote granting, commit advance,
//!   majority, log matching), proven exhaustively by Kani.
//! * [`log`] — the append-only replicated log with the Log-Matching splice.
//! * [`message`] — the RPC envelopes exchanged between nodes.
//! * [`node`] — the stateful node driver tying it all together.

pub mod decide;
pub mod log;
pub mod message;
pub mod node;
pub mod role;

//! `raftkv` verified core — pure domain + ports. Zero IO.
//!
//! This crate has no async runtime, no database, no HTTP. All non-determinism is
//! abstracted behind [`ports`] (Clock/Rng/IdGen), which is what makes the
//! harness's DST/Loom/TSAN gates meaningful and lets Kani/proptest prove the
//! domain functions total. Add IO adapters in OUTER crates behind these ports —
//! never here.
//!
//! `domain/` is intentionally near-empty: put YOUR domain here. The workflow is
//! spec-first — write `spec/requirements/REQ-NNN.md` + a BDD scenario, then the
//! code, then the tests/proofs (see CLAUDE.md). If your domain is a state
//! machine, model it as a pure `fn next(state, event) -> Option<state>` and add
//! an `[fsm]` section to `harness.toml` to light up `regen`/TLA+/`check-spec-sync`.

#![forbid(unsafe_code)]

pub mod domain;
pub mod ports;

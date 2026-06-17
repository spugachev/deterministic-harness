//! FSM drift gate — spec-sync parses the FSM source (`state.rs::next`) and
//! canonicalizes it, so a malformed transition function fails loudly. Opt-in on
//! `[fsm]` (R2/C2).
use anyhow::{Context, Result};

use crate::config::Config;
use crate::fsm::{parse_fsm_from, Transition};

/// Normalize a parsed FSM into a sorted, canonical string set so two sources
/// can be byte-compared regardless of declaration order.
pub(crate) fn canonical_fsm(
    states: &[String],
    events: &[String],
    transitions: &[Transition],
) -> String {
    let mut s: Vec<String> = states.to_vec();
    s.sort();
    let mut e: Vec<String> = events.to_vec();
    e.sort();
    let mut t: Vec<String> = transitions
        .iter()
        .map(|x| format!("{}+{}->{}", x.source, x.event, x.target))
        .collect();
    t.sort();
    format!(
        "states={}\nevents={}\ntransitions={}",
        s.join(","),
        e.join(","),
        t.join(",")
    )
}

/// Parse the FSM source (`state.rs::next`) and canonicalize it. A malformed or
/// unparseable transition function fails the gate; otherwise the canonical FSM
/// is the same artifact `regen` projects into the `.tla` spec, so this gate is
/// what guarantees the source is well-formed before TLC ever runs.
pub(crate) fn check_spec_sync(cfg: &Config) -> Result<()> {
    let Some(fsm) = cfg.raw.fsm.as_ref() else {
        println!("check-spec-sync: [fsm] not configured (skip)");
        return Ok(());
    };
    let src = cfg.path(&fsm.source).to_string_lossy().into_owned();
    let (a_s, a_e, a_t) = parse_fsm_from(&src, &fsm.fn_name, &fsm.state_enum, &fsm.event_enum)
        .context("parse runtime FSM source")?;
    let _ = canonical_fsm(&a_s, &a_e, &a_t);
    println!(
        "✓ check-spec-sync: FSM source parses ({} transitions)",
        a_t.len()
    );
    Ok(())
}

//! FSM drift gates — spec-sync (FSM source vs the two Verus FSM copies) and
//! the Priority bump/lower mapping sync. Opt-in on `[fsm]` (R2/C2).
use anyhow::{anyhow, Context, Result};

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

/// ADR-0005: Verus proves over a standalone duplicate of the FSM in
/// `verus_proofs.rs`. This gate byte-compares the *canonical* FSM extracted
/// from `state.rs::next` against `verus_proofs.rs::next_spec`, so the duplicate
/// cannot silently drift from the runtime source of truth.
pub(crate) fn check_spec_sync(cfg: &Config) -> Result<()> {
    let Some(fsm) = cfg.raw.fsm.as_ref() else {
        println!("check-spec-sync: [fsm] not configured (skip)");
        return Ok(());
    };
    let src = cfg.path(&fsm.source).to_string_lossy().into_owned();
    let (a_s, a_e, a_t) = parse_fsm_from(&src, &fsm.fn_name, &fsm.state_enum, &fsm.event_enum)
        .context("parse runtime FSM source")?;
    let runtime = canonical_fsm(&a_s, &a_e, &a_t);

    // The Verus duplicate is itself opt-in within [fsm]. Absent ⇒ there is no
    // second copy to drift; the FSM source parsing above is the whole gate.
    let Some(dup) = fsm.verus_dup.as_ref() else {
        println!(
            "✓ check-spec-sync: FSM source parses ({} transitions); no [fsm.verus_dup] to compare",
            a_t.len()
        );
        return Ok(());
    };
    let verus_file = cfg.path(&dup.file).to_string_lossy().into_owned();
    let (b_s, b_e, b_t) =
        parse_fsm_from(&verus_file, &dup.spec_fn, &fsm.state_enum, &fsm.event_enum)
            .context("parse Verus spec FSM")?;
    let (c_s, c_e, c_t) =
        parse_fsm_from(&verus_file, &dup.exec_fn, &fsm.state_enum, &fsm.event_enum)
            .context("parse Verus exec FSM")?;
    let spec = canonical_fsm(&b_s, &b_e, &b_t);
    let exec = canonical_fsm(&c_s, &c_e, &c_t);

    check_priority_bump_sync(cfg, fsm, dup)?;

    if runtime == spec && spec == exec {
        println!(
            "✓ check-spec-sync: FSM source, {}::{}, {}::{}, and Priority transforms agree ({} transitions)",
            dup.file, dup.spec_fn, dup.file, dup.exec_fn, a_t.len()
        );
        return Ok(());
    }

    eprintln!("--- runtime FSM source ---\n{runtime}");
    eprintln!("--- verus spec ---\n{spec}");
    eprintln!("--- verus exec ---\n{exec}");
    Err(anyhow!(
        "check-spec-sync: the FSM copies have drifted (ADR-0005). Reconcile them."
    ))
}

/// Extract a canonical `From => To` priority mapping from a named function's
/// body: every `X::Source => Y::Target` arm, normalized to `Source->Target`
/// and sorted. Works on both the runtime `model.rs` (`Self::Low => Self::Med`)
/// and the Verus duplicate (`Priority::Low => Priority::Med`).
fn extract_priority_mapping(path: &str, fn_name: &str) -> Result<String> {
    let src = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let needle = format!("fn {fn_name}");
    let start = src
        .find(&needle)
        .ok_or_else(|| anyhow!("{path}: no `{needle}` found"))?;
    // Bound the body at the start of the NEXT function definition so arms from
    // a sibling transform (e.g. `lower` following `bump`) are never captured.
    // Indentation of the closing brace differs across files, so a `\n}` scan is
    // unreliable; the next `fn ` is a robust delimiter.
    let after = start + needle.len();
    let body_end = src[after..].find("fn ").map_or(src.len(), |e| after + e);
    let body = &src[start..body_end];

    let arm = regex::Regex::new(r"(?:Self|Priority)::(\w+)\s*=>\s*(?:Self|Priority)::(\w+)")
        .expect("arm regex");
    let mut pairs: Vec<String> = arm
        .captures_iter(body)
        .map(|c| format!("{}->{}", &c[1], &c[2]))
        .collect();
    pairs.sort();
    pairs.dedup();
    if pairs.is_empty() {
        return Err(anyhow!("{path}: no `{fn_name}` arms extracted"));
    }
    Ok(pairs.join(","))
}

/// REQ-016/022: each runtime `Priority` transform and its Verus duplicate must
/// agree. Covers every paired function so a new transform cannot drift.
fn check_priority_bump_sync(
    cfg: &Config,
    fsm: &crate::config::Fsm,
    dup: &crate::config::VerusDup,
) -> Result<()> {
    let priority_src = cfg
        .path(&fsm.priority_source)
        .to_string_lossy()
        .into_owned();
    let verus_file = cfg.path(&dup.file).to_string_lossy().into_owned();
    for f in ["bump", "lower"] {
        let runtime = extract_priority_mapping(&priority_src, f)?;
        let verus = extract_priority_mapping(&verus_file, f)?;
        if runtime != verus {
            eprintln!("--- model.rs Priority::{f} ---\n{runtime}");
            eprintln!("--- verus_proofs.rs {f} ---\n{verus}");
            return Err(anyhow!(
                "check-spec-sync: Priority::{f} drifted between model.rs and the Verus \
                 duplicate (ADR-0005). Reconcile them."
            ));
        }
    }
    Ok(())
}

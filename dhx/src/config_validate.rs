//! `harness.toml` validation, split from `config.rs` to stay within the
//! ≤400-line budget dhx enforces on itself (G4).
//!
//! Three checks: schema version (C6), required fields, and two teeth rules —
//! presence ⇒ mandatory FSM shape (R2/C2) and crate-name cross-check against
//! `cargo metadata` (C14), so a typo fails at load, not mid-run.

use anyhow::{bail, Result};

use crate::config::{Config, SCHEMA_VERSION};

pub(crate) fn validate(cfg: &Config) -> Result<()> {
    // C6 — schema gate with an actionable message.
    let v = cfg.raw.meta.schema_version;
    if v == 0 {
        bail!(
            "harness.toml is missing [meta].schema_version — add `schema_version = {SCHEMA_VERSION}` \
             under a [meta] table (this build of dhx speaks schema {SCHEMA_VERSION})"
        );
    }
    if v != SCHEMA_VERSION {
        bail!(
            "harness.toml schema_version = {v} but this dhx speaks {SCHEMA_VERSION} — run \
             `dhx migrate` (or align your dhx version; see .harness/pins/dhx.txt)"
        );
    }
    if cfg.raw.project.name.trim().is_empty() {
        bail!("harness.toml [project].name is required and must be non-empty");
    }
    if cfg.raw.coverage.core.is_empty() {
        bail!(
            "harness.toml [coverage].core is required — list the verified-core crate(s) held \
             to the high coverage bar"
        );
    }
    validate_fsm_shape(cfg)?;
    validate_crate_names(cfg)?;
    Ok(())
}

/// If the project looks FSM-shaped on disk but isn't configured, fail loudly
/// instead of silently skipping the FSM/spec-sync gates (R2/C2). The trigger is
/// the conventional FSM source path existing — a *source* artifact, never the
/// generated `.tla` (which may not exist before first `regen`).
fn validate_fsm_shape(cfg: &Config) -> Result<()> {
    if cfg.raw.fsm.is_some() {
        return Ok(());
    }
    let conventional = cfg.root.join("crates/core/src/domain/state.rs");
    if conventional.exists() {
        bail!(
            "FSM source {} exists but harness.toml has no [fsm] section — configure it \
             (regen/check-spec-sync would otherwise silently verify nothing)",
            conventional.display()
        );
    }
    Ok(())
}

/// C14: cross-check every crate name referenced in `[coverage].core` and
/// `[targets]` against `cargo metadata` workspace members. An unknown name (a
/// typo, a renamed crate) is a load-time error, not a confusing runtime
/// `cargo -p` failure three gates later.
fn validate_crate_names(cfg: &Config) -> Result<()> {
    let members: std::collections::HashSet<String> = match cfg.workspace_members() {
        Ok(m) => m.into_iter().map(|c| c.name).collect(),
        // No resolvable workspace yet (e.g. mid-`init`, before the first build).
        // Skip — the gates that need metadata will surface it.
        Err(_) => return Ok(()),
    };
    let mut refs: Vec<(&str, &str)> = Vec::new();
    for c in &cfg.raw.coverage.core {
        refs.push(("[coverage].core", c));
    }
    for (key, val) in [
        ("[targets].miri", &cfg.raw.targets.miri),
        ("[targets].tsan", &cfg.raw.targets.tsan),
        ("[targets].loom", &cfg.raw.targets.loom),
    ] {
        if let Some(name) = val {
            refs.push((key, name));
        }
    }
    if let Some(dst) = &cfg.raw.targets.dst {
        refs.push(("[targets].dst.crate", &dst.krate));
    }
    for (key, name) in refs {
        if !members.contains(name) {
            let mut known: Vec<&String> = members.iter().collect();
            known.sort();
            bail!(
                "{key} names crate {name:?}, which is not a workspace member — \
                 fix the typo. Known crates: {known:?}"
            );
        }
    }
    Ok(())
}

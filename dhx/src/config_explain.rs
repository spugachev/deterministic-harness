//! `dhx config explain <gate>` — print a gate's resolved value + provenance
//! (R9). Auto-discovered values (the coverage ignore-set) are flagged so a
//! developer can confirm them in `harness.toml` rather than trusting a silent
//! guess. Split from `config.rs` to keep it within the ≤400 budget (G4).

use anyhow::{bail, Result};

use crate::config::Config;

pub(crate) fn explain(cfg: &Config, gate: &str) -> Result<()> {
    match gate {
        "coverage" => {
            println!(
                "coverage.core        = {:?}  (from harness.toml)",
                cfg.raw.coverage.core
            );
            println!(
                "fail_under_lines     = {}  (default applies unless set)",
                cfg.raw.coverage.fail_under_lines
            );
            let others: Vec<String> = cfg
                .workspace_members()?
                .into_iter()
                .filter(|m| !cfg.raw.coverage.core.contains(&m.name))
                .map(|m| m.name)
                .collect();
            println!(
                "ignored crates       = {others:?}  (auto-discovered from cargo metadata — confirm)"
            );
        }
        "targets" => {
            println!("miri  = {:?}", cfg.raw.targets.miri);
            println!("tsan  = {:?}", cfg.raw.targets.tsan);
            println!("loom  = {:?}", cfg.raw.targets.loom);
            println!("fuzz  = {:?}", cfg.raw.targets.fuzz);
        }
        "fsm" => match &cfg.raw.fsm {
            Some(f) => println!(
                "fsm.source = {}  fn = {}  (from harness.toml)",
                f.source, f.fn_name
            ),
            None => println!("fsm = (not configured — regen/spec-sync skip)"),
        },
        "docs" => {
            println!("requirements_dir = {}", cfg.raw.docs.requirements_dir);
            println!("spec_dir         = {}", cfg.raw.docs.spec_dir);
            println!("req_id_pattern   = {}", cfg.raw.docs.req_id_pattern);
        }
        other => bail!("unknown gate {other:?} — try: coverage | targets | fsm | docs"),
    }
    Ok(())
}

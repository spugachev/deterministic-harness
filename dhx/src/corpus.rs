//! The single source of truth for filesystem walks the gates share (C11).
//!
//! Before this module, the REQ-doc walk, the `.feature` scenario parse, and the
//! `.tla` spec discovery were each duplicated — and *diverged* — across
//! `traceability`, `bdd_gates`, `docs_gates`, and `tlc`. Divergence is a
//! silent-toothlessness vector: one loop honours config, another keeps a
//! literal. Every gate now calls into here, so a path/pattern lives in exactly
//! one place.

use std::path::PathBuf;

use anyhow::Result;

use crate::config::Config;
use crate::traceability::{read_frontmatter, ReqFrontmatter};

/// One Gherkin scenario: the REQ id parsed from its title + concatenated
/// lowercased title/step text, for acceptance-coverage matching.
pub(crate) struct Scenario {
    pub(crate) req: Option<String>,
    pub(crate) text: String,
}

/// The REQ-id regex, compiled once from `[docs].req_id_pattern` and consumed by
/// every id call site (so the configurable scheme is real, not partial).
pub(crate) fn req_id_re(cfg: &Config) -> regex::Regex {
    regex::Regex::new(&cfg.raw.docs.req_id_pattern)
        .unwrap_or_else(|e| panic!("invalid [docs].req_id_pattern: {e}"))
}

/// All active+other REQ frontmatters under the requirements dir (skips
/// `non-goals.md`). The single REQ walk; replaces the three divergent copies.
pub(crate) fn requirements(cfg: &Config) -> Result<Vec<(PathBuf, ReqFrontmatter)>> {
    let dir = cfg.path(&cfg.raw.docs.requirements_dir);
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in walkdir::WalkDir::new(&dir).max_depth(1) {
        let e = entry?;
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("md")
            || p.file_name().and_then(|x| x.to_str()) == Some("non-goals.md")
        {
            continue;
        }
        let fm: ReqFrontmatter = read_frontmatter(p)?;
        out.push((p.to_path_buf(), fm));
    }
    Ok(out)
}

/// Every scenario across the features dir. The single `.feature` parse;
/// replaces both `collect_scenarios` and the count-only walk in docs-counts.
pub(crate) fn scenarios(cfg: &Config) -> Result<Vec<Scenario>> {
    let dir = cfg.path(&cfg.raw.docs.features_dir);
    let req_re = req_id_re(cfg);
    let mut out: Vec<Scenario> = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in walkdir::WalkDir::new(&dir) {
        let e = entry?;
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("feature") {
            continue;
        }
        let raw = std::fs::read_to_string(p)?;
        for line in raw.lines() {
            let t = line.trim();
            if let Some(rest) = t
                .strip_prefix("Scenario:")
                .or_else(|| t.strip_prefix("Scenario Outline:"))
            {
                let req = req_re.find(rest).map(|m| m.as_str().to_owned());
                out.push(Scenario {
                    req,
                    text: rest.to_lowercase(),
                });
            } else if let Some(s) = out.last_mut() {
                if ["Given", "When", "Then", "And", "But"]
                    .iter()
                    .any(|k| t.starts_with(k))
                {
                    s.text.push(' ');
                    s.text.push_str(&t.to_lowercase());
                }
            }
        }
    }
    Ok(out)
}

/// Feature-file count + scenario count (for docs-counts), from the one walk.
pub(crate) fn scenario_counts(cfg: &Config) -> Result<(u32, u32)> {
    let dir = cfg.path(&cfg.raw.docs.features_dir);
    let (mut files, mut scen) = (0_u32, 0_u32);
    if !dir.exists() {
        return Ok((0, 0));
    }
    for entry in walkdir::WalkDir::new(&dir) {
        let e = entry?;
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("feature") {
            continue;
        }
        files = files.saturating_add(1);
        let raw = std::fs::read_to_string(p)?;
        for l in raw.lines() {
            let t = l.trim();
            if t.starts_with("Scenario:") || t.starts_with("Scenario Outline:") {
                scen = scen.saturating_add(1);
            }
        }
    }
    Ok((files, scen))
}

/// TLA specs partitioned into the FSM-generated stem vs the manual specs
/// (manual = all `*.tla` minus `{generated_stem}.tla`), resolving the old
/// hardcoded `["tla/ConcurrentApi.tla"]` list (C10).
pub(crate) struct TlaSpecs {
    pub(crate) generated: Vec<PathBuf>,
    pub(crate) manual: Vec<PathBuf>,
}

pub(crate) fn tla_specs(cfg: &Config) -> Result<TlaSpecs> {
    let dir = cfg.path(&cfg.raw.docs.spec_dir);
    let generated_name = cfg
        .raw
        .fsm
        .as_ref()
        .map(|f| format!("{}.tla", f.generated_stem));
    let mut generated = Vec::new();
    let mut manual = Vec::new();
    if !dir.exists() {
        return Ok(TlaSpecs { generated, manual });
    }
    for entry in walkdir::WalkDir::new(&dir).max_depth(1) {
        let e = entry?;
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("tla") {
            continue;
        }
        let name = p.file_name().and_then(|x| x.to_str()).unwrap_or_default();
        if generated_name.as_deref() == Some(name) {
            generated.push(p.to_path_buf());
        } else {
            manual.push(p.to_path_buf());
        }
    }
    Ok(TlaSpecs { generated, manual })
}

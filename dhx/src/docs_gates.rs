//! Cheap structural gates: file-size limit + README docs-counts.
use anyhow::{anyhow, Context, Result};

use crate::config::Config;
use crate::corpus;

const FILE_SIZE_LIMIT: usize = 400;

/// Every tracked `.rs` under a workspace member crate must be ≤
/// [`FILE_SIZE_LIMIT`] lines. Long files hide complexity the per-function
/// `too_many_lines` clippy lint can't see. No escape hatch — an oversized file
/// is split, not excused. Roots come from `cargo metadata` (C9/R7), not a
/// literal `crates/`, so a project with a non-`crates/` layout is still walked.
pub(crate) fn check_file_size(cfg: &Config) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();
    let mut checked = 0_u32;

    for member in cfg.workspace_members()? {
        for entry in walkdir::WalkDir::new(&member.manifest_dir) {
            let e = entry?;
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("rs")
                || p.components().any(|c| c.as_os_str() == "target")
            {
                continue;
            }
            let rel = p
                .strip_prefix(&cfg.root)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/");
            let lines = std::fs::read_to_string(p)?.lines().count();
            checked = checked.saturating_add(1);
            if lines > FILE_SIZE_LIMIT {
                errors.push(format!(
                    "{rel}: {lines} lines > {FILE_SIZE_LIMIT} limit — split it into modules"
                ));
            }
        }
    }

    if !errors.is_empty() {
        for e in &errors {
            eprintln!("  ✗ {e}");
        }
        return Err(anyhow!(
            "check-file-size: {} file(s) over the {FILE_SIZE_LIMIT}-line limit",
            errors.len()
        ));
    }
    println!("✓ check-file-size OK ({checked} files ≤ {FILE_SIZE_LIMIT} lines)");
    Ok(())
}

/// Keep README counts honest by rewriting a *marked region* rather than
/// matching brittle English prose (R7). The README must contain:
///
/// ```text
/// <!-- dhx:counts -->
/// (anything; regenerated)
/// <!-- /dhx:counts -->
/// ```
///
/// In `--check` mode (the gate) the region's content must equal the freshly
/// computed line; otherwise it is rewritten in place. Counts: `.feature`
/// scenarios, feature files, and the highest REQ id — all from [`corpus`], so
/// they cannot diverge from the other gates.
pub(crate) fn check_docs_counts(cfg: &Config) -> Result<()> {
    docs_counts(cfg, true)
}

pub(crate) fn write_docs_counts(cfg: &Config) -> Result<()> {
    docs_counts(cfg, false)
}

const OPEN: &str = "<!-- dhx:counts -->";
const CLOSE: &str = "<!-- /dhx:counts -->";

fn docs_counts(cfg: &Config, check: bool) -> Result<()> {
    let (feature_files, scenarios) = corpus::scenario_counts(cfg)?;

    // Highest REQ-NNN id (numeric suffix) under the requirements dir.
    let mut max_req = 0_u32;
    let num_re = regex::Regex::new(r"(\d+)").expect("num regex");
    for (path, fm) in corpus::requirements(cfg)? {
        let _ = path;
        if let Some(c) = num_re.captures(&fm.id) {
            max_req = max_req.max(c[1].parse().unwrap_or(0));
        }
    }

    let line = format!(
        "{scenarios} Gherkin scenario(s) across {feature_files} feature file(s); requirements up to REQ-{max_req:03}."
    );

    let readme_path = cfg.path(&cfg.raw.docs.readme);
    if !readme_path.exists() {
        println!("check-docs-counts: no {} (skip)", cfg.raw.docs.readme);
        return Ok(());
    }
    let readme = std::fs::read_to_string(&readme_path).context("read README")?;

    let (Some(open_at), Some(close_at)) = (readme.find(OPEN), readme.find(CLOSE)) else {
        return Err(anyhow!(
            "{}: missing the `{OPEN} … {CLOSE}` counts region — add it where the counts should live",
            cfg.raw.docs.readme
        ));
    };
    let body_start = open_at + OPEN.len();
    if close_at < body_start {
        return Err(anyhow!(
            "{}: {CLOSE} appears before {OPEN}",
            cfg.raw.docs.readme
        ));
    }
    let current = readme[body_start..close_at].trim();

    if check {
        if current != line {
            return Err(anyhow!(
                "check-docs-counts: README counts region drifted.\n  have: {current:?}\n  want: {line:?}\n  run `dhx check-docs-counts --write` (or `dhx regen`)",
            ));
        }
        println!("✓ check-docs-counts OK ({line})");
    } else {
        let new = format!(
            "{}{OPEN}\n{line}\n{CLOSE}{}",
            &readme[..open_at],
            &readme[close_at + CLOSE.len()..]
        );
        std::fs::write(&readme_path, new)?;
        println!("✓ check-docs-counts wrote counts ({line})");
    }
    Ok(())
}

//! Requirements traceability gate + frontmatter parsing, split from main.rs.
use anyhow::{anyhow, Context, Result};

use crate::config::Config;
use crate::corpus;

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ReqFrontmatter {
    pub(crate) id: String,
    #[allow(dead_code, reason = "kept for future title/state validation")]
    title: String,
    pub(crate) status: String,
    pub(crate) acceptance: Vec<String>,
    #[serde(default)]
    pub(crate) implements_in: std::collections::BTreeMap<String, Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
struct AdrFrontmatter {
    id: String,
    #[allow(dead_code, reason = "kept for future")]
    title: String,
    status: String,
    #[serde(default)]
    implements: Vec<String>,
}

/// Does `file`'s contents contain a *definition* of `sym` (not merely a
/// mention)? A bare whole-word token match passes on a lingering comment after
/// the real definition was renamed/removed, which defeats the rename-protection
/// the traceability gate advertises. We require definition syntax instead:
///   * `.tla` — a top-level operator `Sym ==` or `Sym(args) ==` at line start.
///     The `\b` after the name excludes a sibling like `UpdateRejected` when the
///     link points at `Update`.
///   * Rust/other — a `fn|struct|enum|trait|const|static|type|mod Sym` item
///     (covers REQ-007's `model.rs::Version` tuple struct and the proof `fn`s).
pub(crate) fn symbol_is_defined(file: &str, sym: &str, contents: &str) -> bool {
    let q = regex::escape(sym);
    let is_tla = std::path::Path::new(file)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("tla"));
    let pattern = if is_tla {
        format!(r"(?m)^\s*{q}\b\s*(\([^)]*\))?\s*==")
    } else {
        format!(r"\b(fn|struct|enum|trait|const|static|type|mod)\s+{q}\b")
    };
    regex::Regex::new(&pattern)
        .ok()
        .is_some_and(|re| re.is_match(contents))
}

pub(crate) fn read_frontmatter<T: for<'de> serde::Deserialize<'de>>(
    path: &std::path::Path,
) -> Result<T> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let stripped = raw
        .strip_prefix("---\n")
        .ok_or_else(|| anyhow!("{}: no YAML frontmatter", path.display()))?;
    let end = stripped
        .find("\n---")
        .ok_or_else(|| anyhow!("{}: unterminated YAML frontmatter", path.display()))?;
    let yaml = &stripped[..end];
    serde_yaml::from_str(yaml)
        .with_context(|| format!("parse YAML frontmatter of {}", path.display()))
}

pub(crate) fn check_traceability(cfg: &Config) -> Result<()> {
    traceability(cfg, false)
}

/// `--write` mode: rewrite the lock instead of verifying it (C4). Used by `init`
/// and `dhx check-traceability --write`.
pub(crate) fn write_traceability(cfg: &Config) -> Result<()> {
    traceability(cfg, true)
}

#[allow(
    clippy::too_many_lines,
    reason = "single linear validator; readable as a list"
)]
fn traceability(cfg: &Config, write_lock: bool) -> Result<()> {
    use std::collections::BTreeMap;

    let req_dir = cfg.path(&cfg.raw.docs.requirements_dir);
    let adr_dir = cfg.path(&cfg.raw.docs.adr_dir);
    if !req_dir.exists() || !adr_dir.exists() {
        println!("check-traceability: no requirements or adr dir yet (skip)");
        return Ok(());
    }

    // Anchored id patterns from config (the `^…$` is added so the whole id
    // must match, not just a substring).
    let req_id_re = regex::Regex::new(&format!("^{}$", cfg.raw.docs.req_id_pattern))
        .context("compile [docs].req_id_pattern")?;
    let adr_id_re = regex::Regex::new(&format!("^{}$", cfg.raw.docs.adr_id_pattern))
        .context("compile [docs].adr_id_pattern")?;

    // Collect REQs (the single walk, via corpus).
    let mut reqs: BTreeMap<String, ReqFrontmatter> = BTreeMap::new();
    for (p, fm) in corpus::requirements(cfg)? {
        if !req_id_re.is_match(&fm.id) {
            return Err(anyhow!(
                "{}: id {:?} does not match {}",
                p.display(),
                fm.id,
                cfg.raw.docs.req_id_pattern
            ));
        }
        if fm.acceptance.is_empty() {
            return Err(anyhow!("{}: REQ has empty acceptance list", p.display()));
        }
        reqs.insert(fm.id.clone(), fm);
    }
    println!("check-traceability: parsed {} REQs", reqs.len());

    // Collect ADRs.
    let mut adrs: BTreeMap<String, AdrFrontmatter> = BTreeMap::new();
    for entry in walkdir::WalkDir::new(&adr_dir).max_depth(1) {
        let e = entry?;
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("md") {
            continue;
        }
        let fm: AdrFrontmatter = read_frontmatter(p)?;
        if !adr_id_re.is_match(&fm.id) {
            return Err(anyhow!(
                "{}: id {:?} does not match {}",
                p.display(),
                fm.id,
                cfg.raw.docs.adr_id_pattern
            ));
        }
        adrs.insert(fm.id.clone(), fm);
    }
    println!("check-traceability: parsed {} ADRs", adrs.len());

    // ADR.implements references must exist as REQs.
    for (id, adr) in &adrs {
        if adr.status != "accepted" && adr.status != "proposed" && adr.status != "superseded" {
            return Err(anyhow!(
                "{}: status {:?} not in (proposed,accepted,superseded)",
                id,
                adr.status
            ));
        }
        for r in &adr.implements {
            if !reqs.contains_key(r) {
                return Err(anyhow!("{} implements unknown REQ {}", id, r));
            }
        }
    }

    // Each active REQ must have at least one implements_in entry.
    for (id, req) in &reqs {
        if req.status != "active" && req.status != "non-goal" && req.status != "superseded" {
            return Err(anyhow!(
                "{}: status {:?} not in (active,non-goal,superseded)",
                id,
                req.status
            ));
        }
        if req.status == "active" && req.implements_in.is_empty() {
            return Err(anyhow!("{}: active REQ has no implements_in entries", id));
        }
        // Every path listed under implements_in (code/gherkin/tla/...) must
        // exist on disk — a dangling link is a traceability lie. Entries may
        // be `path` or `path::symbol` (e.g. a TLA+ action or Rust item); we
        // validate the file part AND, when a `::symbol` is given, that the
        // symbol appears as a whole-word token in that file (so a rename can't
        // leave a stale link silently passing).
        for (kind, paths) in &req.implements_in {
            for p in paths {
                let (file, symbol) = p
                    .split_once("::")
                    .map_or((p.as_str(), None), |(f, s)| (f, Some(s)));
                let abs = cfg.path(file);
                if !abs.exists() {
                    return Err(anyhow!(
                        "{id}: implements_in.{kind} references missing path {file:?}"
                    ));
                }
                if let Some(sym) = symbol {
                    let contents = std::fs::read_to_string(&abs)
                        .with_context(|| format!("{id}: read {file} to check symbol {sym:?}"))?;
                    // Require a DEFINITION, not just a token mention, so a
                    // renamed/removed symbol can't stay green on a stale comment.
                    if !symbol_is_defined(file, sym, &contents) {
                        return Err(anyhow!(
                            "{id}: implements_in.{kind} references {p:?} but no definition \
                             of {sym:?} is present in {file} (renamed/removed?)"
                        ));
                    }
                }
            }
        }
    }

    // ADR-0009: every Action in a MANUAL .tla module must be REQ-annotated.
    check_manual_tla_annotations(cfg)?;

    // traceability.lock.json — diff in this file = "what changed in requirements".
    // C4: in `--check` mode (the default for `dhx check`/the hook/the read-only
    // container) we VERIFY the committed lock matches; only `--write` rewrites
    // it, so the gate never dirties a read-only tree.
    let snapshot: BTreeMap<&String, &Vec<String>> =
        reqs.iter().map(|(k, v)| (k, &v.acceptance)).collect();
    let json = format!("{}\n", serde_json::to_string_pretty(&snapshot)?);
    let lock_path = cfg.path(&cfg.raw.docs.traceability_lock);
    if write_lock {
        std::fs::write(&lock_path, &json)?;
        println!("check-traceability: wrote {}", lock_path.display());
    } else {
        let on_disk = std::fs::read_to_string(&lock_path).unwrap_or_default();
        if on_disk != json {
            return Err(anyhow!(
                "{}: traceability lock is stale — run `dhx check-traceability --write` and commit it",
                cfg.raw.docs.traceability_lock
            ));
        }
    }

    println!("✓ check-traceability OK");
    Ok(())
}

/// ADR-0009 consequence: in a MANUAL `.tla` module every Action — a top-level
/// *parameterized* operator `Name(args) == …`, which is how this codebase
/// writes its atomic-apply steps (`Create(c,id)`, `Update(c,id,expected)`, …) —
/// must carry a `\* REQ-NNN` comment on the operator line or the line directly
/// above it. Zero-arg operators are invariants/properties/helpers (`TypeOK`,
/// `TombstoneSticks`, `Init`, `Next`, `Spec`, …), not Actions, so they are
/// exempt. This makes the per-Action traceability the ADR claims actually
/// enforced rather than conventional.
fn check_manual_tla_annotations(cfg: &Config) -> Result<()> {
    // Manual specs are auto-derived (C10): every `.tla` in the spec dir that is
    // NOT the FSM-generated stem. No hardcoded file list.
    let manual = corpus::tla_specs(cfg)?.manual;
    if manual.is_empty() {
        return Ok(());
    }
    let action_re =
        regex::Regex::new(r"^([A-Za-z][A-Za-z0-9_]*)\s*\([^)]*\)\s*==").expect("action regex");
    let req_re = regex::Regex::new(&cfg.raw.docs.req_id_pattern).context("req id pattern")?;

    for file in &manual {
        let file = file.display().to_string();
        let contents = std::fs::read_to_string(&file)
            .with_context(|| format!("check-traceability: read manual spec {file}"))?;
        let lines: Vec<&str> = contents.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let Some(caps) = action_re.captures(line) else {
                continue;
            };
            let name = caps.get(1).map_or("", |m| m.as_str());
            // The REQ tag may sit on the operator line itself, or on any of the
            // contiguous comment lines immediately above it (the common style).
            let on_line = req_re.is_match(line);
            let mut above = false;
            let mut j = i;
            while j > 0 {
                let prev = lines[j - 1].trim_start();
                if prev.starts_with("\\*") {
                    if req_re.is_match(prev) {
                        above = true;
                        break;
                    }
                    j -= 1;
                } else {
                    break;
                }
            }
            if !on_line && !above {
                return Err(anyhow!(
                    "{file}: Action {name:?} has no `\\* REQ-NNN` annotation on its \
                     line or the comment block above it (ADR-0009 requires every \
                     Action in a manual .tla module to be REQ-traceable)"
                ));
            }
        }
    }
    println!("✓ check-traceability: manual .tla Actions are REQ-annotated");
    Ok(())
}

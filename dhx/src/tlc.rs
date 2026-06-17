//! TLA+/TLC model-checking gates + anti-vacuity mutation testing.
//!
//! Specs are discovered from `[docs].spec_dir` (no hardcoded list). The
//! anti-vacuity mutation table (R5/C2) is a PROJECT data file
//! `<spec_dir>/mutations.toml`, not compiled-in — the `find`/`replace` strings
//! are fragile textual patches coupled to a specific spec, so they belong with
//! the specs they mutate. Absent file ⇒ the mutate/coverage gates are no-ops,
//! UNLESS a `.cfg` declares invariants (then coverage FAILS: specs present but
//! anti-vacuity unconfigured — the banned silent-toothless case).
use crate::config::Config;
use crate::corpus;
use crate::run;
use crate::toolchain::{assert_tool_version, read_pin};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn resolve_tlc_jar(cfg: &Config, required: bool) -> Result<Option<String>> {
    let jar = std::env::var("TLA2TOOLS").ok().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        let candidates = [
            format!("{home}/.local/lib/tla2tools.jar"),
            "/usr/local/lib/tla2tools.jar".to_owned(),
            "/usr/local/bin/tla2tools.jar".to_owned(),
        ];
        candidates
            .iter()
            .find(|p| PathBuf::from(p).exists())
            .cloned()
            .unwrap_or_else(|| candidates[0].clone())
    });
    if !PathBuf::from(&jar).exists() {
        if required {
            return Err(anyhow!(
                "tla2tools.jar not found at {jar} — TLC is a required `verify --full` \
                 gate and cannot be silently skipped. Install per the README or set $TLA2TOOLS."
            ));
        }
        eprintln!("tla2tools.jar missing at {jar}; skipping (set $TLA2TOOLS)");
        return Ok(None);
    }
    // Enforce the pinned TLC version so the jar a developer runs is the one the
    // specs were checked against — the TLC banner prints `Version <n> of ...`.
    let pin = read_pin(cfg.path(".harness/pins/tla2tools.txt"))?;
    assert_tool_version(
        "tlc",
        Command::new("java").args(["-cp", &jar, "tlc2.TLC", "-h"]),
        &pin,
    )?;
    Ok(Some(jar))
}

pub(crate) fn tlc(cfg: &Config, required: bool) -> Result<()> {
    let specs = corpus::tla_specs(cfg)?;
    let all: Vec<PathBuf> = specs.generated.into_iter().chain(specs.manual).collect();
    if all.is_empty() {
        println!("tlc: no .tla specs (skip)");
        return Ok(());
    }
    let Some(jar) = resolve_tlc_jar(cfg, required)? else {
        return Ok(());
    };
    for spec in &all {
        let stem = spec.with_extension("");
        let cfg_path = spec.with_extension("cfg");
        let mut c = Command::new("java");
        c.current_dir(&cfg.root);
        c.args([
            "-cp",
            &jar,
            "tlc2.TLC",
            "-deadlock",
            "-workers",
            "auto",
            "-config",
            &cfg_path.to_string_lossy(),
            &spec.to_string_lossy(),
        ]);
        run(&format!("tlc {}", stem.display()), &mut c)?;
    }
    Ok(())
}

/// One anti-vacuity mutation, loaded from `mutations.toml`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TlcMutation {
    /// Spec basename under the spec dir (e.g. `ConcurrentApi`).
    pub(crate) spec: String,
    /// Human label for the mutation.
    pub(crate) label: String,
    /// Substring to find in the `.tla` (must occur exactly once).
    pub(crate) find: String,
    /// Replacement that breaks the property `expect`.
    pub(crate) replace: String,
    /// The invariant/property name TLC must report as violated.
    pub(crate) expect: String,
}

/// An invariant intentionally without a mutation, with a justifying reason.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MutationExempt {
    pub(crate) spec: String,
    pub(crate) name: String,
    pub(crate) reason: String,
}

/// The `mutations.toml` data file: the project's anti-vacuity table.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct MutationTable {
    #[serde(default)]
    pub(crate) mutations: Vec<TlcMutation>,
    #[serde(default)]
    pub(crate) exempt: Vec<MutationExempt>,
}

/// Load `<spec_dir>/mutations.toml`. Missing file ⇒ an empty table.
pub(crate) fn load_mutations(cfg: &Config) -> Result<MutationTable> {
    let path = cfg.path(&format!("{}/mutations.toml", cfg.raw.docs.spec_dir));
    if !path.exists() {
        return Ok(MutationTable::default());
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

/// Run one mutation through TLC in an isolated temp dir.
fn run_one_mutation(cfg: &Config, m: &TlcMutation, jar: &str, tmp: &Path) -> Result<(), String> {
    let spec_tla = cfg.path(&format!("{}/{}.tla", cfg.raw.docs.spec_dir, m.spec));
    let spec_cfg = cfg.path(&format!("{}/{}.cfg", cfg.raw.docs.spec_dir, m.spec));
    let tla_src =
        std::fs::read_to_string(&spec_tla).map_err(|e| format!("{}: read .tla: {e}", m.spec))?;
    let occurrences = tla_src.matches(&m.find).count();
    if occurrences != 1 {
        return Err(format!(
            "{}: mutation {:?} find-string matched {} times (expected 1) — spec changed, \
             update mutations.toml",
            m.spec, m.label, occurrences
        ));
    }
    let mutated = tla_src.replace(&m.find, &m.replace);

    let _ = std::fs::remove_dir_all(tmp);
    let setup = || -> Result<()> {
        std::fs::create_dir_all(tmp)?;
        std::fs::write(tmp.join(format!("{}.tla", m.spec)), &mutated)?;
        std::fs::copy(&spec_cfg, tmp.join(format!("{}.cfg", m.spec)))?;
        Ok(())
    };
    setup().map_err(|e: anyhow::Error| format!("{}: temp setup: {e}", m.spec))?;

    let out = Command::new("java")
        .args([
            "-cp",
            jar,
            "tlc2.TLC",
            "-deadlock",
            "-workers",
            "auto",
            "-config",
            &format!("{}.cfg", m.spec),
            &format!("{}.tla", m.spec),
        ])
        .current_dir(tmp)
        .output()
        .map_err(|e| format!("{}: run TLC: {e}", m.label))?;

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() && text.contains(&m.expect) {
        println!(
            "  ✓ {} / {} → {} violated (non-vacuous)",
            m.spec, m.label, m.expect
        );
        Ok(())
    } else if out.status.success() {
        Err(format!(
            "{} / {}: TLC stayed GREEN under a mutation that should break {} — \
             the invariant is VACUOUS (constrains nothing)",
            m.spec, m.label, m.expect
        ))
    } else {
        Err(format!(
            "{} / {}: TLC failed but did not report {} (likely a parse error, not the \
             intended violation) — fix the mutation:\n{}",
            m.spec,
            m.label,
            m.expect,
            text.lines().rev().take(8).collect::<Vec<_>>().join("\n")
        ))
    }
}

pub(crate) fn tlc_mutate(cfg: &Config, required: bool) -> Result<()> {
    let table = load_mutations(cfg)?;
    if table.mutations.is_empty() {
        println!("tlc --mutate: no mutations declared (skip)");
        return Ok(());
    }
    let Some(jar) = resolve_tlc_jar(cfg, required)? else {
        return Ok(());
    };
    let tmp = std::env::temp_dir().join(format!("{}-tlc-mutate", cfg.raw.project.name));
    let failures: Vec<String> = table
        .mutations
        .iter()
        .filter_map(|m| run_one_mutation(cfg, m, &jar, &tmp).err())
        .collect();
    let _ = std::fs::remove_dir_all(&tmp);

    if !failures.is_empty() {
        for f in &failures {
            eprintln!("  ✗ {f}");
        }
        return Err(anyhow!(
            "tlc --mutate: {} vacuity check(s) failed",
            failures.len()
        ));
    }
    println!(
        "✓ tlc --mutate OK ({} invariants/properties proven non-vacuous)",
        table.mutations.len()
    );
    Ok(())
}

/// Every `(spec, invariant-or-property)` declared across the spec dir's `.cfg`s.
pub(crate) fn declared_cfg_invariants(cfg: &Config) -> Result<Vec<(String, String)>> {
    let dir = cfg.path(&cfg.raw.docs.spec_dir);
    let line_re =
        regex::Regex::new(r"^\s*(?:INVARIANT|PROPERTY)\s+(\w+)\s*$").expect("cfg invariant regex");
    let mut declared: Vec<(String, String)> = Vec::new();
    if !dir.exists() {
        return Ok(declared);
    }
    for entry in walkdir::WalkDir::new(&dir).max_depth(1) {
        let e = entry?;
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("cfg") {
            continue;
        }
        let spec = p
            .file_stem()
            .and_then(|x| x.to_str())
            .ok_or_else(|| anyhow!("non-utf8 cfg name"))?
            .to_owned();
        for line in std::fs::read_to_string(p)?.lines() {
            if let Some(c) = line_re.captures(line) {
                declared.push((spec.clone(), c[1].to_owned()));
            }
        }
    }
    Ok(declared)
}

pub(crate) fn check_mutation_coverage(cfg: &Config) -> Result<()> {
    let declared = declared_cfg_invariants(cfg)?;
    if declared.is_empty() {
        println!("check-mutation-coverage: no .cfg invariants (skip)");
        return Ok(());
    }
    let table = load_mutations(cfg)?;

    let mutated: std::collections::HashSet<(&str, &str)> = table
        .mutations
        .iter()
        .map(|m| (m.spec.as_str(), m.expect.as_str()))
        .collect();
    let exempt: std::collections::HashSet<(&str, &str)> = table
        .exempt
        .iter()
        .map(|e| (e.spec.as_str(), e.name.as_str()))
        .collect();

    let mut errors: Vec<String> = Vec::new();
    let mut covered = 0_u32;
    let mut exempted = 0_u32;

    for (spec, name) in &declared {
        let key = (spec.as_str(), name.as_str());
        if mutated.contains(&key) {
            covered = covered.saturating_add(1);
        } else if exempt.contains(&key) {
            exempted = exempted.saturating_add(1);
        } else {
            errors.push(format!(
                "{spec}: invariant/property {name:?} has no mutation proving it non-vacuous \
                 and no exemption — add a [[mutations]] entry that breaks it, or an [[exempt]] \
                 entry with a reason, in {}/mutations.toml",
                cfg.raw.docs.spec_dir
            ));
        }
    }

    let declared_set: std::collections::HashSet<(&str, &str)> = declared
        .iter()
        .map(|(s, n)| (s.as_str(), n.as_str()))
        .collect();
    for e in &table.exempt {
        if !declared_set.contains(&(e.spec.as_str(), e.name.as_str())) {
            errors.push(format!(
                "mutations.toml exempts {}/{} but no .cfg declares it — remove the stale \
                 exemption (reason was: {})",
                e.spec, e.name, e.reason
            ));
        }
    }

    if !errors.is_empty() {
        for e in &errors {
            eprintln!("  ✗ {e}");
        }
        return Err(anyhow!(
            "check-mutation-coverage: {} invariant(s) lack anti-vacuity coverage",
            errors.len()
        ));
    }
    println!("✓ check-mutation-coverage OK ({covered} invariants mutated, {exempted} exempted)");
    Ok(())
}

//! `harness.toml` → resolved [`Config`] (the one project-specific manifest dhx
//! reads), plus shape-detection that keeps gates honest.
//!
//! Resolution is two layers only (R9): an explicit `harness.toml` value, else a
//! typed default. `Config::load()` walks up from the cwd to the first
//! `harness.toml`, validates `schema_version` (C6), and applies defaults.
//!
//! **Presence ⇒ mandatory (R2/C2):** a gate whose *source* input exists on disk
//! but is unconfigured is a load error, never a silent skip — the project's
//! worst documented failure mode is the toothless gate. Shape is detected from
//! disk, independent of the config section being policed.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

/// The schema version this build of `dhx` understands. A `harness.toml` with a
/// different `schema_version` is a hard, actionable error (C6) rather than a
/// confusing missing-field failure.
pub(crate) const SCHEMA_VERSION: u32 = 1;

/// Raw deserialization of `harness.toml`. Every section is optional at the parse
/// layer; required-ness is enforced in [`Config::validate`] so the error names
/// the missing section instead of a serde line/column.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawConfig {
    #[serde(default)]
    pub(crate) meta: Meta,
    #[serde(default)]
    pub(crate) project: Project,
    #[serde(default)]
    pub(crate) docs: Docs,
    #[serde(default)]
    pub(crate) coverage: Coverage,
    #[serde(default)]
    pub(crate) configs: Configs,
    #[serde(default)]
    pub(crate) targets: Targets,
    pub(crate) fsm: Option<Fsm>,
    pub(crate) verus: Option<Verus>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Meta {
    #[serde(default)]
    pub(crate) schema_version: u32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Project {
    #[serde(default)]
    pub(crate) name: String,
}

/// Documentation/methodology paths. All have convention defaults so a clean
/// scaffold needs only the non-default keys (C16).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Docs {
    #[serde(default = "d_requirements")]
    pub(crate) requirements_dir: String,
    #[serde(default = "d_adr")]
    pub(crate) adr_dir: String,
    #[serde(default = "d_features")]
    pub(crate) features_dir: String,
    #[serde(default = "d_spec")]
    pub(crate) spec_dir: String,
    #[serde(default = "d_lock")]
    pub(crate) traceability_lock: String,
    #[serde(default = "d_readme")]
    pub(crate) readme: String,
    #[serde(default = "d_req_id")]
    pub(crate) req_id_pattern: String,
    #[serde(default = "d_adr_id")]
    pub(crate) adr_id_pattern: String,
}

fn d_requirements() -> String {
    "spec/requirements".to_owned()
}
fn d_adr() -> String {
    "spec/adr".to_owned()
}
fn d_features() -> String {
    "spec/features".to_owned()
}
fn d_spec() -> String {
    "spec/tla".to_owned()
}
fn d_lock() -> String {
    "spec/traceability.lock.json".to_owned()
}
fn d_readme() -> String {
    "README.md".to_owned()
}
fn d_req_id() -> String {
    r"REQ-\d{3}".to_owned()
}
fn d_adr_id() -> String {
    r"ADR-\d{4}".to_owned()
}

impl Default for Docs {
    fn default() -> Self {
        Self {
            requirements_dir: d_requirements(),
            adr_dir: d_adr(),
            features_dir: d_features(),
            spec_dir: d_spec(),
            traceability_lock: d_lock(),
            readme: d_readme(),
            req_id_pattern: d_req_id(),
            adr_id_pattern: d_adr_id(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Coverage {
    /// The high-bar "verified core" crate(s). Everything else in the workspace
    /// is excluded from the coverage report (replaces the hardcoded
    /// ignore-regex). Required; validated against `cargo metadata` (C14).
    #[serde(default)]
    pub(crate) core: Vec<String>,
    #[serde(default = "d_cov90")]
    pub(crate) fail_under_lines: u32,
    #[serde(default = "d_cov90")]
    pub(crate) fail_under_functions: u32,
}

fn d_cov90() -> u32 {
    90
}

impl Default for Coverage {
    fn default() -> Self {
        Self {
            core: Vec::new(),
            fail_under_lines: 90,
            fail_under_functions: 90,
        }
    }
}

/// Relocated tool-config paths (all four tools accept an explicit path flag).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Configs {
    #[serde(default = "d_deny")]
    pub(crate) deny: String,
    #[serde(default = "d_gitleaks")]
    pub(crate) gitleaks: String,
    #[serde(default = "d_mutants")]
    pub(crate) mutants: String,
    /// Reserved: the scaffold relocates `nextest.toml` here for when the test
    /// gate standardizes on nextest (today `test()` uses `cargo test`).
    #[serde(default = "d_nextest")]
    #[allow(dead_code, reason = "reserved for the nextest test runner")]
    pub(crate) nextest: String,
}

fn d_deny() -> String {
    ".harness/config/deny.toml".to_owned()
}
fn d_gitleaks() -> String {
    ".harness/config/gitleaks.toml".to_owned()
}
fn d_mutants() -> String {
    ".harness/config/mutants.toml".to_owned()
}
fn d_nextest() -> String {
    ".harness/config/nextest.toml".to_owned()
}

impl Default for Configs {
    fn default() -> Self {
        Self {
            deny: d_deny(),
            gitleaks: d_gitleaks(),
            mutants: d_mutants(),
            nextest: d_nextest(),
        }
    }
}

/// Per-gate crate/test "roles" — the thing `cargo metadata` cannot infer (R1),
/// so these are required for the role gates.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Targets {
    pub(crate) miri: Option<String>,
    pub(crate) tsan: Option<String>,
    pub(crate) loom: Option<String>,
    pub(crate) dst: Option<TestTarget>,
    #[serde(default)]
    pub(crate) fuzz: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestTarget {
    #[serde(rename = "crate")]
    pub(crate) krate: String,
    pub(crate) test: String,
}

/// FSM-shaped domain. Presence enables `regen` + `check-spec-sync`; detected
/// from `source` existing on disk (C2).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fsm {
    pub(crate) source: String,
    pub(crate) fn_name: String,
    pub(crate) priority_source: String,
    pub(crate) state_enum: String,
    pub(crate) event_enum: String,
    pub(crate) generated_stem: String,
    pub(crate) verus_dup: Option<VerusDup>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerusDup {
    pub(crate) file: String,
    pub(crate) spec_fn: String,
    pub(crate) exec_fn: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Verus {
    pub(crate) entry: String,
}

/// The resolved, validated configuration plus the project root it was found in.
/// Every gate takes `&Config`; paths are resolved relative to [`Config::root`].
#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) root: PathBuf,
    pub(crate) raw: RawConfig,
}

impl Config {
    /// Find `harness.toml` by walking up from `cwd`, parse, validate, return it.
    pub(crate) fn load() -> Result<Self> {
        let cwd = std::env::current_dir().context("get current dir")?;
        let root = find_root(&cwd).ok_or_else(|| {
            anyhow!(
                "no harness.toml found from {} upward — run `dhx init <path>` to scaffold a \
                 project, or create harness.toml",
                cwd.display()
            )
        })?;
        Self::load_from(&root)
    }

    /// Load + validate from an explicit project root (used by `dhx init` after
    /// it materializes the scaffold).
    pub(crate) fn load_from(root: &Path) -> Result<Self> {
        let manifest = root.join("harness.toml");
        let text = std::fs::read_to_string(&manifest)
            .with_context(|| format!("read {}", manifest.display()))?;
        let raw: RawConfig =
            toml::from_str(&text).with_context(|| format!("parse {}", manifest.display()))?;
        let cfg = Self {
            root: root.to_path_buf(),
            raw,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// Resolve a config-relative path against the project root.
    pub(crate) fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn validate(&self) -> Result<()> {
        // C6 — schema gate with an actionable message.
        let v = self.raw.meta.schema_version;
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
        if self.raw.project.name.trim().is_empty() {
            bail!("harness.toml [project].name is required and must be non-empty");
        }
        if self.raw.coverage.core.is_empty() {
            bail!(
                "harness.toml [coverage].core is required — list the verified-core crate(s) held \
                 to the high coverage bar"
            );
        }
        // R2/C2 — presence ⇒ mandatory, keyed off SOURCE on disk, never off the
        // config section being policed.
        self.validate_fsm_shape()?;
        Ok(())
    }

    /// If the project looks FSM-shaped on disk but isn't configured, fail loudly
    /// instead of silently skipping the FSM/spec-sync gates (R2/C2). The trigger
    /// is the conventional FSM source path existing — a *source* artifact, never
    /// the generated `.tla` (which may not exist before first `regen`).
    fn validate_fsm_shape(&self) -> Result<()> {
        if self.raw.fsm.is_some() {
            return Ok(());
        }
        // Heuristic shape probe, independent of [fsm]: a state.rs with a
        // `fn next` under any crate's domain dir is the deterministic-harness
        // FSM convention.
        let conventional = self.root.join("crates/core/src/domain/state.rs");
        if conventional.exists() {
            bail!(
                "FSM source {} exists but harness.toml has no [fsm] section — configure it \
                 (regen/check-spec-sync would otherwise silently verify nothing)",
                conventional.display()
            );
        }
        Ok(())
    }

    /// Workspace member crate names, from `cargo metadata` (cached by cargo
    /// itself). Used for coverage ignore derivation + [`validate_targets`].
    pub(crate) fn workspace_members(&self) -> Result<Vec<MemberCrate>> {
        let meta = cargo_metadata::MetadataCommand::new()
            .current_dir(&self.root)
            .no_deps()
            .exec()
            .context("run cargo metadata")?;
        let ws: std::collections::HashSet<_> = meta.workspace_members.iter().cloned().collect();
        Ok(meta
            .packages
            .into_iter()
            .filter(|p| ws.contains(&p.id))
            .map(|p| MemberCrate {
                name: p.name,
                manifest_dir: p
                    .manifest_path
                    .parent()
                    .map(|d| d.as_std_path().to_path_buf())
                    .unwrap_or_default(),
            })
            .collect())
    }
}

/// A workspace member crate: its name and the directory holding its `Cargo.toml`.
#[derive(Debug, Clone)]
pub(crate) struct MemberCrate {
    pub(crate) name: String,
    pub(crate) manifest_dir: PathBuf,
}

fn find_root(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if d.join("harness.toml").is_file() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

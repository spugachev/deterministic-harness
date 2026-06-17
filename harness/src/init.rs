//! `dhx init <path>` — scaffold a new deterministic-harness service.
//!
//! Materializes the embedded `assets/scaffold/` tree into the target dir,
//! stripping the inert `dot`-prefixes and `.tmpl`/`.template` suffixes (G1/G2)
//! and substituting `{{project}}`/version placeholders. The embedded assets are
//! data inside the crate (R4/F4), so they survive `cargo package`/`cargo
//! install`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use include_dir::{include_dir, Dir};

/// The scaffold, embedded at compile time from inside the crate dir.
static SCAFFOLD: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets/scaffold");

pub(crate) fn run(path: &str, name: Option<&str>, force: bool) -> Result<()> {
    let target = expand(path);
    let project = name
        .map(ToOwned::to_owned)
        .or_else(|| {
            target
                .file_name()
                .and_then(|n| n.to_str())
                .map(ToOwned::to_owned)
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("could not derive a project name from {path:?}; pass --name"))?;

    if which("cargo").is_none() {
        bail!("`cargo` not found on PATH — dhx init needs cargo to generate the FSM spec");
    }

    std::fs::create_dir_all(&target).with_context(|| format!("create {}", target.display()))?;
    if !force && dir_nonempty(&target)? {
        bail!(
            "{} is not empty — refusing to scaffold over it (use --force to override)",
            target.display()
        );
    }

    println!("dhx init: scaffolding {project} into {}", target.display());
    materialize(&SCAFFOLD, &target, &project)?;

    // git init (C8) if not already a repo, then point hooks at the harness dir.
    if which("git").is_some() {
        if !target.join(".git").exists() {
            run_in(&target, "git", &["init", "-q"])?;
        }
        run_in(
            &target,
            "git",
            &["config", "core.hooksPath", ".harness/hooks"],
        )?;
    } else {
        println!("  (git not found — skipped repo init + hooks; install git and run `dhx init` flags later)");
    }

    // Generate the lifecycle TLA from the scaffolded FSM source (C5), plus the
    // README counts region + traceability lock, so the fresh project is green
    // immediately. (There is no per-project Dockerfile — every tier runs through
    // the single `dhx:latest` image built from the deterministic-harness repo.)
    let cfg = crate::config::Config::load_from(&target)?;
    crate::fsm::regen(&cfg, false)?;
    crate::docs_gates::write_docs_counts(&cfg)?;
    crate::traceability::write_traceability(&cfg).ok();

    println!(
        "\n✓ dhx init done. Next (all tiers run inside the dhx image):\n  \
         cd {}\n  \
         docker run --rm -v \"$PWD\":/work -w /work dhx:latest dhx check\n  \
         docker run --rm -v \"$PWD\":/work -w /work dhx:latest dhx verify --quick\n  \
         docker run --rm -v \"$PWD\":/work -w /work dhx:latest dhx verify --full",
        target.display()
    );
    Ok(())
}

/// Materialize the embedded scaffold into `dest` (rename + substitute only — no
/// git/cargo/regen). Test-only seam so dhx can self-verify that its shipped
/// scaffold is structurally valid (C-EX) without the full `init` ceremony.
#[cfg(test)]
pub(crate) fn materialize_to(dest: &Path, project: &str) -> Result<()> {
    materialize(&SCAFFOLD, dest, project)
}

/// Recursively write an embedded dir, renaming inert names and substituting.
fn materialize(dir: &Dir<'_>, dest_root: &Path, project: &str) -> Result<()> {
    for file in dir.files() {
        let rel = file.path();
        let out_rel = rename(rel);
        let out = dest_root.join(&out_rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = file.contents();
        if let Ok(text) = std::str::from_utf8(bytes) {
            let rendered = text.replace("{{project}}", project);
            std::fs::write(&out, rendered).with_context(|| format!("write {}", out.display()))?;
        } else {
            std::fs::write(&out, bytes).with_context(|| format!("write {}", out.display()))?;
        }
    }
    for sub in dir.dirs() {
        materialize(sub, dest_root, project)?;
    }
    Ok(())
}

/// Strip inert prefixes/suffixes from each path component:
/// `dot.cargo` → `.cargo`, `dotgitignore` → `.gitignore`, `Cargo.toml.tmpl` →
/// `Cargo.toml`, `CLAUDE.md.template` → `CLAUDE.md`. The `_shared/` dir is an
/// internal include source and is dropped.
fn rename(rel: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in rel.components() {
        let mut s = comp.as_os_str().to_string_lossy().into_owned();
        if let Some(rest) = s.strip_prefix("dot.") {
            s = format!(".{rest}");
        } else if let Some(rest) = s.strip_prefix("dot") {
            // `dotgitignore` → `.gitignore`
            s = format!(".{rest}");
        }
        s = s
            .strip_suffix(".tmpl")
            .or_else(|| s.strip_suffix(".template"))
            .map_or(s.clone(), ToOwned::to_owned);
        out.push(s);
    }
    out
}

fn expand(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

fn dir_nonempty(p: &Path) -> Result<bool> {
    Ok(std::fs::read_dir(p)?.next().is_some())
}

fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let cand = dir.join(bin);
        cand.is_file().then_some(cand)
    })
}

fn run_in(dir: &Path, program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .current_dir(dir)
        .args(args)
        .status()
        .with_context(|| format!("run {program} {}", args.join(" ")))?;
    if !status.success() {
        bail!("{program} {} failed: {status}", args.join(" "));
    }
    Ok(())
}

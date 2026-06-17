//! Thin wrappers around the external verification tools, split from `main.rs`.
//!
//! Every command runs with `current_dir(cfg.root)` so dhx works from any
//! subdirectory; paths/targets/pins all come from [`Config`].
use crate::config::Config;
use crate::toolchain::{assert_tool_version, read_pin};
use crate::{run, try_run};
use anyhow::{anyhow, Result};
use std::process::Command;

/// A `cargo`-family command rooted at the project. Use for every spawn so the
/// gate is cwd-independent. Shared with `tools_heavy`.
pub(crate) fn at_root(cfg: &Config, program: &str) -> Command {
    let mut c = Command::new(program);
    c.current_dir(&cfg.root);
    c
}

pub(crate) fn fmt(cfg: &Config, check: bool) -> Result<()> {
    let mut c = at_root(cfg, "cargo");
    c.args(["fmt", "--all"]);
    if check {
        c.arg("--").arg("--check");
    }
    run(
        if check {
            "cargo fmt --check"
        } else {
            "cargo fmt"
        },
        &mut c,
    )
}

pub(crate) fn clippy(cfg: &Config) -> Result<()> {
    let mut c = at_root(cfg, "cargo");
    c.args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    ]);
    run(
        "cargo clippy --workspace --all-targets --all-features -- -D warnings",
        &mut c,
    )
}

pub(crate) fn test(cfg: &Config, changed_since: Option<String>) -> Result<()> {
    let mut c = at_root(cfg, "cargo");
    c.args(["test", "--workspace", "--all-features"]);
    if let Some(rev) = changed_since {
        // honest: nextest --partition by hash; not real TIA
        c.args(["--", "--include-ignored"])
            .env("CHANGED_SINCE", rev);
    }
    run("cargo test --workspace", &mut c)
}

pub(crate) fn cov(cfg: &Config) -> Result<()> {
    // Coverage gate on the "verified core" crate(s) from `[coverage].core`. We
    // run the WHOLE workspace suite (`--workspace`) — the core's use-cases are
    // driven mainly by the integration/BDD tests of *other* crates, so running
    // only the core's own unit tests leaves it artificially low — then restrict
    // the *report* to the core via `--ignore-filename-regex` built from every
    // OTHER workspace member's dir. The ignore-set is derived from
    // `cargo metadata` (not a hardcoded crate list, which silently rots), so it
    // stays correct as crates are added/renamed. Lines+functions bar (default
    // 90%) rather than regions: residual uncovered regions are typically the
    // `arbitrary::Arbitrary` impls exercised only by the excluded fuzz crate.
    let core: std::collections::HashSet<&str> =
        cfg.raw.coverage.core.iter().map(String::as_str).collect();
    let mut ignored: Vec<String> = Vec::new();
    for m in cfg.workspace_members()? {
        if !core.contains(m.name.as_str()) {
            let rel = m
                .manifest_dir
                .strip_prefix(&cfg.root)
                .unwrap_or(&m.manifest_dir)
                .to_string_lossy()
                .replace('\\', "/");
            ignored.push(regex::escape(&format!("{rel}/")));
        }
    }
    let ignore_re = format!("({})", ignored.join("|"));
    let lines = cfg.raw.coverage.fail_under_lines.to_string();
    let funcs = cfg.raw.coverage.fail_under_functions.to_string();
    let mut c = at_root(cfg, "cargo");
    c.args(["llvm-cov", "--workspace", "--all-features"]);
    if !ignored.is_empty() {
        c.args(["--ignore-filename-regex", &ignore_re]);
    }
    c.args([
        "--fail-under-lines",
        &lines,
        "--fail-under-functions",
        &funcs,
    ]);
    if !try_run(
        "cargo llvm-cov (verified core, driven by the full suite)",
        &mut c,
    ) {
        return Err(anyhow!(
            "cargo-llvm-cov missing or core line/function coverage below threshold"
        ));
    }
    Ok(())
}

pub(crate) fn mutants(cfg: &Config, shard: Option<String>, baseline: Option<String>) -> Result<()> {
    // NOTE: deliberately NOT `--in-place`. cargo-mutants defaults to mutating
    // a temp copy of the tree; `--in-place` edits the real sources, and if the
    // run is interrupted it leaves a mutant on disk that can be committed by
    // accident (this happened once — see commit history). The temp-copy
    // default is slower but cannot corrupt the working tree.
    let mut c = at_root(cfg, "cargo");
    c.args(["mutants"]);
    let mutants_cfg = cfg.path(&cfg.raw.configs.mutants);
    if mutants_cfg.exists() {
        c.arg("--config").arg(&mutants_cfg);
    }
    if let Some(s) = shard {
        c.arg("--shard").arg(s);
    }
    if let Some(b) = baseline {
        c.arg("--baseline").arg(b);
    }
    if !try_run("cargo mutants", &mut c) {
        return Err(anyhow!("cargo-mutants failed or below threshold"));
    }
    Ok(())
}

pub(crate) fn kani(cfg: &Config) -> Result<()> {
    // `--harness-timeout` is a BACKSTOP against a genuinely *intractable*
    // harness (CBMC unrolling that blows up — e.g. an unbounded Vec/String
    // equality loop, which made `idempotency_classify_total` run >540s before
    // its `#[kani::unwind(3)]` bound took it to ~3s). It is deliberately NOT a
    // performance assertion: a tight bound would be load-sensitive and could
    // turn CPU starvation into a false RED, violating verdict determinism
    // (docs/intent-drift.md). Every healthy harness solves in <5s, so 300s is
    // ~60x headroom — only a true runaway (minutes→hours) trips it, and then it
    // fails LOUDLY by name instead of hanging. (Requires -Z unstable-options.)
    let mut c = at_root(cfg, "cargo");
    c.args([
        "kani",
        "-Z",
        "unstable-options",
        "--harness-timeout",
        "300s",
        "--workspace",
    ]);
    if !try_run(
        "cargo kani --workspace (per-harness backstop timeout 300s)",
        &mut c,
    ) {
        return Err(anyhow!(
            "Kani not installed, a harness timed out, or proofs failed. A timeout names the \
             harness — bound its loops with `#[kani::unwind(N)]` or reduce its symbolic input. \
             Install: `cargo install kani-verifier && cargo kani setup`"
        ));
    }
    Ok(())
}

/// Compile the `#[cfg(kani)]` harnesses WITHOUT running CBMC (`--only-codegen`,
/// ~10s). The cheap `dhx check`/`cargo build`/`cargo clippy` paths never
/// build the Kani crate, so a Kani-only compile break (e.g. a `let _ = (..)` on
/// a `Drop` type tripping `-D let_underscore_drop`, which silently broke the
/// whole harness crate in iteration 26) stays invisible until the slow
/// `verify --full` Kani run. This gate surfaces that in seconds. Runs in
/// `verify --quick` just before the full `kani()` so a compile break fails fast
/// instead of after minutes of codegen+CBMC.
pub(crate) fn check_kani_codegen(cfg: &Config) -> Result<()> {
    let mut c = at_root(cfg, "cargo");
    c.args([
        "kani",
        "-Z",
        "unstable-options",
        "--only-codegen",
        "--workspace",
    ]);
    if !try_run("cargo kani --only-codegen (Kani harnesses compile)", &mut c) {
        return Err(anyhow!(
            "Kani harnesses failed to COMPILE (or Kani is not installed). This is a \
             Kani-only break invisible to `cargo build`/`clippy` — fix the #[cfg(kani)] \
             code in your `#[cfg(kani)]` proof module. Install: `cargo install kani-verifier && cargo kani setup`"
        ));
    }
    Ok(())
}

pub(crate) fn verus(cfg: &Config) -> Result<()> {
    // [verus] absent here means the project genuinely has no Verus proofs — and
    // `config_validate::validate_verus_shape` has already hard-failed at load if
    // a conventional `verus_proofs.rs` exists unconfigured (presence ⇒ mandatory,
    // R2/C2). So this skip is safe, not silent.
    let Some(verus) = cfg.raw.verus.as_ref() else {
        println!("verus: [verus] not configured (skip — no deductive proofs in this project)");
        return Ok(());
    };
    // Verus has no stable releases, so pin the exact build by version and assert
    // it before proving — two machines on different SHAs (different Z3 +
    // prelude) can reach different verdicts. Pin lives at the configured path.
    let pin = read_pin(cfg.path(".harness/pins/verus.txt"))?;
    assert_tool_version("verus", at_root(cfg, "verus").arg("--version"), &pin)?;
    let mut c = at_root(cfg, "verus");
    c.args([verus.entry.as_str(), "--crate-type=lib"]);
    if !try_run(&format!("verus (pinned {pin}) {}", verus.entry), &mut c) {
        return Err(anyhow!("Verus proofs failed"));
    }
    Ok(())
}

pub(crate) fn deny(cfg: &Config) -> Result<()> {
    // `--config` is a flag of the `check` SUBCOMMAND, not the top level — at the
    // top level `-c` means `--color`. So `check` comes first, then `--config`.
    let mut c = at_root(cfg, "cargo");
    c.args(["deny", "check"]);
    let deny_cfg = cfg.path(&cfg.raw.configs.deny);
    if deny_cfg.exists() {
        c.arg("--config").arg(&deny_cfg);
    }
    if !try_run("cargo deny check", &mut c) {
        return Err(anyhow!("cargo-deny not installed or violations found"));
    }
    Ok(())
}

pub(crate) fn machete(cfg: &Config) -> Result<()> {
    // Invoke the `cargo-machete` binary directly rather than via the
    // `cargo machete` subcommand shim: when dhx itself runs under `cargo run`,
    // the nested cargo mangles the `--with-metadata` flag into a positional
    // path argument. Calling the binary avoids that.
    let mut c = at_root(cfg, "cargo-machete");
    c.args(["--with-metadata", "--skip-target-dir"]);
    if !try_run("cargo-machete --with-metadata", &mut c) {
        return Err(anyhow!("cargo-machete not installed or unused deps found"));
    }
    Ok(())
}

pub(crate) fn outdated(cfg: &Config, major_only: bool) -> Result<()> {
    // soft warn: never fails the verify run; just prints
    let mut c = at_root(cfg, "cargo");
    c.args(["outdated", "--depth", "1"]);
    if major_only {
        c.args(["--root-deps-only"]);
    }
    let _ok = try_run("cargo outdated (soft warn)", &mut c);
    Ok(())
}

/// `cargo geiger` is **advisory, not a gate** — its result is intentionally
/// NOT asserted. The real `unsafe`-code guarantee is `#![forbid(unsafe_code)]`
/// in every crate (a compile error if violated, far stronger than a count), and
/// geiger notoriously miscounts `unsafe` in third-party deps you cannot fix. It
/// runs only to surface a dependency-tree `unsafe` *trend* for a human to eye.
/// Labeled "(advisory, non-gating)" so it is never mistaken for verification —
/// the honest alternative to a check that looks like a gate but can't fail.
pub(crate) fn geiger(cfg: &Config) -> Result<()> {
    let mut c = at_root(cfg, "cargo");
    c.args(["geiger"]);
    let ran = try_run("cargo geiger (advisory, non-gating)", &mut c);
    if !ran {
        println!("ℹ cargo geiger unavailable or non-zero — advisory only, not failing the run");
    }
    Ok(())
}

pub(crate) fn gitleaks(cfg: &Config) -> Result<()> {
    // gitleaks >= 8.19 renamed `detect` to `git`. Scan tracked git history
    // (not the working tree / target dir) against the configured allowlist.
    let mut c = at_root(cfg, "gitleaks");
    c.args(["git", "--no-banner"]);
    let gl_cfg = cfg.path(&cfg.raw.configs.gitleaks);
    if gl_cfg.exists() {
        c.arg("-c").arg(&gl_cfg);
    }
    if !try_run("gitleaks git", &mut c) {
        return Err(anyhow!("gitleaks not installed or secrets found"));
    }
    Ok(())
}

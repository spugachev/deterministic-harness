//! dhx — Deterministic Harness: opinionated scaffolder + single CLI
//! orchestrating all verification gates for a new Rust service.
//!
//! `dhx init <path>` scaffolds a project in the deterministic-harness
//! architecture; `dhx check` (cheap, daily loop), `dhx verify --quick`, and
//! `dhx verify --full` (everything, in-container) are the entire verification
//! story, run locally. There is no CI.

#![allow(
    clippy::missing_docs_in_private_items,
    clippy::disallowed_macros,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::module_name_repetitions,
    clippy::disallowed_methods,
    clippy::unnecessary_wraps,
    clippy::uninlined_format_args,
    clippy::option_if_let_else,
    clippy::single_match_else,
    clippy::needless_pass_by_ref_mut,
    clippy::format_push_string,
    reason = "developer CLI tool"
)]

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};

mod config;
use config::Config;
mod config_explain;
mod config_validate;
mod proc;
pub(crate) use proc::{run, try_run};
mod corpus;
mod docker;
mod init;
mod migrate;
mod tlc;
use tlc::{check_mutation_coverage, tlc, tlc_mutate};
mod toolchain;
mod tools;
use tools::{
    check_kani_codegen, clippy, cov, deny, fmt, geiger, gitleaks, kani, machete, mutants, outdated,
    test, verus,
};
mod tools_heavy;
use tools_heavy::{dst, dst_seeded, fuzz, loom_run, miri, tsan};
mod fsm;
mod fsm_render;
use fsm::regen;
mod fsm_sync;
use fsm_sync::check_spec_sync;
mod traceability;
use traceability::{check_traceability, write_traceability};
mod bdd_style;
use bdd_style::check_bdd_style;
mod bdd_gates;
use bdd_gates::{check_bdd_coverage, check_verified_markers};
mod docs_gates;
use docs_gates::{check_docs_counts, check_file_size, write_docs_counts};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Deterministic Harness — scaffold + verify a Rust service"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Scaffold a new deterministic-harness service at <path>.
    Init {
        /// Target directory (created if absent). Use `.` for the cwd.
        path: String,
        /// Override the project name (default: the dir name).
        #[arg(long)]
        name: Option<String>,
        /// Proceed even if the target dir is non-empty.
        #[arg(long)]
        force: bool,
    },
    /// Inspect resolved config: `dhx config explain <gate>`.
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Migrate harness.toml to the schema this dhx speaks.
    Migrate,
    /// Print/update the pinned tool versions.
    PinsUpdate,
    /// Regenerate the repo-root Dockerfile from the embedded template.
    RenderDockerfile {
        #[arg(long)]
        check: bool,
    },
    Verify {
        #[arg(long)]
        full: bool,
        #[arg(long)]
        quick: bool,
    },
    /// Fast preflight: all cheap deterministic gates, aggregated.
    Check,
    CheckDocsCounts {
        /// Rewrite the README counts region instead of verifying it.
        #[arg(long)]
        write: bool,
    },
    Regen {
        /// Verify the on-disk artefact matches the source instead of writing it.
        #[arg(long)]
        check: bool,
    },
    CheckTraceability {
        /// Rewrite the traceability lock instead of verifying it.
        #[arg(long)]
        write: bool,
    },
    CheckSpecSync,
    CheckBddStyle,
    CheckBddCoverage,
    CheckMutationCoverage,
    CheckVerifiedMarkers,
    CheckFileSize,
    Fmt {
        #[arg(long)]
        check: bool,
    },
    Clippy,
    Test {
        #[arg(long)]
        changed_since: Option<String>,
    },
    Cov,
    Mutants {
        #[arg(long)]
        shard: Option<String>,
        #[arg(long)]
        baseline: Option<String>,
    },
    Kani,
    /// Compile the Kani harnesses without running CBMC (fast Kani-rot check).
    CheckKaniCodegen,
    Verus,
    Fuzz {
        target: Option<String>,
        #[arg(long, default_value_t = 20_000)]
        runs: u32,
    },
    Miri,
    Tsan,
    Loom,
    Dst {
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long, default_value_t = 50_000)]
        iterations: u64,
    },
    Deny,
    Machete,
    Outdated {
        #[arg(long)]
        major_only: bool,
    },
    Geiger,
    Gitleaks,
    Tlc {
        /// Instead of model-checking, verify each invariant/property is
        /// non-vacuous: inject a known-violating mutation and require TLC to
        /// catch it.
        #[arg(long)]
        mutate: bool,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCmd {
    /// Show a gate's resolved value + provenance (gate: coverage|targets|fsm|docs).
    Explain { gate: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // `init` runs BEFORE any harness.toml exists, so it does not load Config.
    if let Cmd::Init { path, name, force } = &cli.cmd {
        return init::run(path, name.as_deref(), *force);
    }
    let cfg = Config::load()?;
    match cli.cmd {
        Cmd::Init { .. } => unreachable!("handled above"),
        Cmd::Config { cmd } => match cmd {
            ConfigCmd::Explain { gate } => config_explain::explain(&cfg, &gate),
        },
        Cmd::Migrate => migrate::run(&cfg),
        Cmd::PinsUpdate => migrate::pins_update(&cfg),
        Cmd::RenderDockerfile { check } => docker::render_dockerfile(&cfg, check),
        Cmd::Verify { full, quick } => verify(&cfg, full, quick),
        Cmd::Check => check_preflight(&cfg),
        Cmd::CheckDocsCounts { write } => {
            if write {
                write_docs_counts(&cfg)
            } else {
                check_docs_counts(&cfg)
            }
        }
        Cmd::Regen { check } => regen(&cfg, check),
        Cmd::CheckTraceability { write } => {
            if write {
                write_traceability(&cfg)
            } else {
                check_traceability(&cfg)
            }
        }
        Cmd::CheckSpecSync => check_spec_sync(&cfg),
        Cmd::CheckBddStyle => check_bdd_style(&cfg),
        Cmd::CheckBddCoverage => check_bdd_coverage(&cfg),
        Cmd::CheckMutationCoverage => check_mutation_coverage(&cfg),
        Cmd::CheckVerifiedMarkers => check_verified_markers(&cfg),
        Cmd::CheckFileSize => check_file_size(&cfg),
        Cmd::Fmt { check } => fmt(&cfg, check),
        Cmd::Clippy => clippy(&cfg),
        Cmd::Test { changed_since } => test(&cfg, changed_since),
        Cmd::Cov => cov(&cfg),
        Cmd::Mutants { shard, baseline } => mutants(&cfg, shard, baseline),
        Cmd::Kani => kani(&cfg),
        Cmd::CheckKaniCodegen => check_kani_codegen(&cfg),
        Cmd::Verus => verus(&cfg),
        Cmd::Fuzz { target, runs } => fuzz(&cfg, target, runs),
        Cmd::Miri => miri(&cfg),
        Cmd::Tsan => tsan(&cfg),
        Cmd::Loom => loom_run(&cfg),
        Cmd::Dst { seed, iterations } => dst(&cfg, seed, iterations),
        Cmd::Deny => deny(&cfg),
        Cmd::Machete => machete(&cfg),
        Cmd::Outdated { major_only } => outdated(&cfg, major_only),
        Cmd::Geiger => geiger(&cfg),
        Cmd::Gitleaks => gitleaks(&cfg),
        Cmd::Tlc { mutate } => {
            if mutate {
                tlc_mutate(&cfg, false)
            } else {
                tlc(&cfg, false)
            }
        }
    }
}

// --- top-level entry points -------------------------------------------------

fn verify(cfg: &Config, full: bool, quick: bool) -> Result<()> {
    if full && quick {
        return Err(anyhow!("--full and --quick are mutually exclusive"));
    }
    let full = full || !quick; // default = full
    if full {
        verify_full(cfg)
    } else {
        verify_quick(cfg)
    }
}

fn verify_quick(cfg: &Config) -> Result<()> {
    println!("== verify --quick ==");
    regen(cfg, true)?;
    check_traceability(cfg)?;
    check_spec_sync(cfg)?;
    check_bdd_style(cfg)?;
    check_bdd_coverage(cfg)?;
    check_verified_markers(cfg)?;
    check_mutation_coverage(cfg)?;
    check_file_size(cfg)?;
    check_docs_counts(cfg)?;
    fmt(cfg, true)?;
    clippy(cfg)?;
    machete(cfg)?;
    gitleaks(cfg)?;
    deny(cfg)?;
    test(cfg, None)?;
    cov(cfg)?;
    check_kani_codegen(cfg)?;
    kani(cfg)?;
    dst(cfg, 0, 2_000)?;
    println!("== verify --quick OK ==");
    Ok(())
}

fn verify_full(cfg: &Config) -> Result<()> {
    // C1: `--full` runs in the pinned container so every external tool matches
    // the pins. If we're NOT already inside the image, re-exec there. No Docker
    // daemon ⇒ hard fail (never a silent host-tool fallback).
    if !docker::in_container() {
        return docker::reexec_full(cfg);
    }
    println!("== verify --full (in container) ==");
    verify_quick(cfg)?;
    outdated(cfg, true)?;
    geiger(cfg)?;
    mutants(cfg, None, None)?;
    verus(cfg)?;
    miri(cfg)?;
    tsan(cfg)?;
    loom_run(cfg)?;
    // DST: fixed seeds are the deterministic regression set; the trailing
    // `random` run is the discovery pass (fresh entropy, seed printed for replay).
    for seed in [0_u64, 1, 42, 1337] {
        dst(cfg, seed, 20_000)?;
    }
    dst_seeded(cfg, "random", 20_000)?;
    for tgt in &cfg.raw.targets.fuzz {
        fuzz(cfg, Some(tgt.clone()), 20_000)?;
    }
    tlc(cfg, true)?;
    tlc_mutate(cfg, true)?;
    println!("== verify --full OK ==");
    Ok(())
}

type Gate = (&'static str, fn(&Config) -> Result<()>);

fn check_preflight(cfg: &Config) -> Result<()> {
    let gates: [Gate; 11] = [
        ("fmt --check", |c| fmt(c, true)),
        ("regen --check", |c| regen(c, true)),
        ("clippy", clippy),
        ("check-traceability", check_traceability),
        ("check-spec-sync", check_spec_sync),
        ("check-bdd-style", check_bdd_style),
        ("check-bdd-coverage", check_bdd_coverage),
        ("check-verified-markers", check_verified_markers),
        ("check-mutation-coverage", check_mutation_coverage),
        ("check-file-size", check_file_size),
        ("check-docs-counts", check_docs_counts),
    ];
    let mut failed: Vec<String> = Vec::new();
    for (name, gate) in gates {
        if let Err(e) = gate(cfg) {
            failed.push(format!("{name}: {e}"));
        }
    }

    if !failed.is_empty() {
        eprintln!("\n── preflight: {} gate(s) failed ──", failed.len());
        for f in &failed {
            eprintln!("  ✗ {f}");
        }
        return Err(anyhow!(
            "preflight failed ({} of {} gates) — fix all above, then re-run `dhx check`",
            failed.len(),
            gates.len()
        ));
    }
    println!(
        "✓ preflight OK ({} cheap gates) — run `dhx verify --quick` for the full gate",
        gates.len()
    );
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

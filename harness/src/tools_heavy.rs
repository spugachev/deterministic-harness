//! Heavy / nightly / container-bound tool wrappers (fuzz, tsan, loom, dst),
//! split from `tools.rs` to keep each file within the ≤400 budget that dhx
//! enforces on itself (G4).
use crate::config::Config;
use crate::toolchain::{host_triple, pinned_nightly};
use crate::tools::at_root;
use crate::try_run;
use anyhow::{anyhow, Result};

pub(crate) fn fuzz(cfg: &Config, target: Option<String>, runs: u32) -> Result<()> {
    // Optional gate: no target given and none configured ⇒ the project has
    // nothing to fuzz. Skip cleanly (a project with raw-input parsers opts in
    // via [targets].fuzz); never hard-fail an otherwise-green tier.
    let Some(target) = target.or_else(|| cfg.raw.targets.fuzz.first().cloned()) else {
        println!("fuzz: [targets].fuzz is empty (skip — no fuzz targets in this project)");
        return Ok(());
    };
    // Fuzzing is a DISCOVERY gate, not a verification gate: it must use fresh
    // entropy each run (libFuzzer's default random seed) so that across many
    // runs it actually explores new inputs and finds real bugs. We do NOT pin
    // `-seed` — a fixed seed would re-test the same paths forever. Determinism
    // is provided by *persistence* instead: a crash is written to
    // `fuzz/artifacts/<target>/` and replayed deterministically as a
    // regression. The `-runs` bound just keeps the gate time-boxed.
    let mut c = at_root(cfg, "cargo");
    c.args([
        "+nightly",
        "fuzz",
        "run",
        &target,
        "--",
        &format!("-runs={runs}"),
    ]);
    if !try_run(&format!("cargo fuzz run {target} -- -runs={runs}"), &mut c) {
        return Err(anyhow!("cargo-fuzz failed or nightly missing"));
    }
    Ok(())
}

/// Resolve an optional single-crate `[targets]` role. `None` ⇒ the gate has no
/// crate to run on; the caller skips cleanly (these targets default to the core
/// in a scaffold, but a project may legitimately have nothing for a given one).
fn target_crate<'a>(role: &str, val: Option<&'a String>) -> Option<&'a str> {
    if val.is_none() {
        println!("{role}: [targets].{role} not configured (skip)");
    }
    val.map(String::as_str)
}

pub(crate) fn tsan(cfg: &Config) -> Result<()> {
    // ThreadSanitizer needs the standard library recompiled with the same
    // `-Zsanitizer=thread` ABI, so we pass `-Zbuild-std` and an explicit
    // target triple. We sanitize `todo-core-memory` (the crate with the
    // actual `RwLock` concurrency), not `todo-api` (whose hyper/turmoil deps
    // are heavy and irrelevant to the data-race question).
    // Pinned nightly (nightly-version.txt): the `-Zsanitizer=thread` ABI and
    // build-std behaviour evolve, so a floating nightly would make this race
    // gate non-deterministic across days.
    let Some(krate) = target_crate("tsan", cfg.raw.targets.tsan.as_ref()) else {
        return Ok(());
    };
    let nightly = pinned_nightly(cfg)?;
    let triple = host_triple();
    let mut c = at_root(cfg, "cargo");
    // `--lib` restricts to the crate's UNIT tests. We deliberately exclude
    // integration tests (`tests/*.rs`) — notably the BDD/cucumber harness, whose
    // `harness = false` runner rejects libtest flags like `--test-threads` and
    // spins up a multi-thread async runtime that is irrelevant to the data-race
    // question and incompatible with the sanitizer's single-thread test mode.
    c.args([
        &format!("+{nightly}"),
        "test",
        "--lib",
        "-Zbuild-std",
        &format!("--target={triple}"),
        "-p",
        krate,
        "--",
        "--test-threads=1",
    ]);
    c.env("RUSTFLAGS", "-Zsanitizer=thread");
    c.env("RUSTDOCFLAGS", "-Zsanitizer=thread");
    if !try_run(&format!("cargo +{nightly} test --lib (TSAN)"), &mut c) {
        return Err(anyhow!(
            "TSAN failed, or pinned nightly {nightly} / rust-src component missing"
        ));
    }
    Ok(())
}

pub(crate) fn loom_run(cfg: &Config) -> Result<()> {
    let Some(krate) = target_crate("loom", cfg.raw.targets.loom.as_ref()) else {
        return Ok(());
    };
    // Presence ⇒ mandatory, else skip: only run when the target crate actually
    // has Loom model tests (`loom::` in its source). `RUSTFLAGS=--cfg loom`
    // propagates to the WHOLE dependency tree, and crates with their own internal
    // `loom` cfg (notably tokio, pulled in as a dev-dependency by the BDD
    // harness) fail to compile under a forced external `--cfg loom`. So a crate
    // with no Loom tests must not trigger this gate at all — running it would
    // only break the build for zero coverage.
    if !crate_uses_loom(cfg, krate) {
        println!(
            "loom: [targets].loom = {krate:?} but it has no `loom::` model tests (skip — \
             add loom tests behind `#[cfg(loom)]` to enable this gate)"
        );
        return Ok(());
    }
    let mut c = at_root(cfg, "cargo");
    // `--lib` only: Loom models the shared-memory unit tests, not the BDD/async
    // integration harness (which `--cfg loom` would not even link correctly).
    c.args(["test", "--lib", "-p", krate, "--release"]);
    c.env("RUSTFLAGS", "--cfg loom");
    if !try_run("cargo test --lib --release (loom)", &mut c) {
        return Err(anyhow!("loom failed"));
    }
    Ok(())
}

/// True if the named workspace crate's `src/` references `loom::` — i.e. it has
/// Loom model tests that justify running the (dependency-tree-wide) `--cfg loom`
/// build. Conservative: any mention counts.
fn crate_uses_loom(cfg: &Config, krate: &str) -> bool {
    let Ok(members) = cfg.workspace_members() else {
        return false;
    };
    let Some(m) = members.iter().find(|m| m.name == krate) else {
        return false;
    };
    let src = m.manifest_dir.join("src");
    for entry in walkdir::WalkDir::new(&src).into_iter().flatten() {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Ok(text) = std::fs::read_to_string(entry.path()) {
                if text.contains("loom::") {
                    return true;
                }
            }
        }
    }
    false
}

pub(crate) fn dst(cfg: &Config, seed: u64, iterations: u64) -> Result<()> {
    dst_seeded(cfg, &seed.to_string(), iterations)
}

/// Run DST with an explicit seed string. `"random"` makes the harness draw a
/// fresh entropy seed and print it for replay — the *discovery* mode. Numeric
/// seeds are the reproducible *regression* mode.
pub(crate) fn dst_seeded(cfg: &Config, seed: &str, iterations: u64) -> Result<()> {
    // Optional gate: no [targets].dst ⇒ the project has no simulation harness
    // yet. Skip cleanly (a service with multi-step/network behaviour opts in by
    // adding a DST integration test and naming it here); never hard-fail a tier.
    let Some(dst) = cfg.raw.targets.dst.as_ref() else {
        println!(
            "dst: [targets].dst not configured (skip — no simulation harness in this project)"
        );
        return Ok(());
    };
    let mut c = at_root(cfg, "cargo");
    c.args([
        "test",
        "-p",
        &dst.krate,
        "--test",
        &dst.test,
        "--",
        "--nocapture",
    ]);
    c.env("DST_SEED", seed)
        .env("DST_ITERATIONS", iterations.to_string());
    if !try_run(&format!("DST seed={seed} iterations={iterations}"), &mut c) {
        return Err(anyhow!(
            "DST failed (seed={seed}) — see the printed reproduce command"
        ));
    }
    Ok(())
}

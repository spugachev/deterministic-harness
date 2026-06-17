//! Heavy / nightly / container-bound tool wrappers (fuzz, miri, tsan, loom,
//! dst), split from `tools.rs` to keep each file within the ≤400 budget that
//! dhx enforces on itself (G4).
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

pub(crate) fn miri(cfg: &Config) -> Result<()> {
    // `-Zmiri-disable-isolation` lets proptest read the clock / cwd it needs;
    // Miri checks the target crate for UB. Pinned nightly: Miri's UB model
    // evolves, so a floating nightly would make this UB gate non-deterministic.
    let Some(krate) = target_crate("miri", cfg.raw.targets.miri.as_ref()) else {
        return Ok(());
    };
    let nightly = pinned_nightly(cfg)?;
    let mut c = at_root(cfg, "cargo");
    c.args([&format!("+{nightly}"), "miri", "test", "-p", krate]);
    c.env("MIRIFLAGS", "-Zmiri-disable-isolation");
    if !try_run(&format!("cargo +{nightly} miri test -p {krate}"), &mut c) {
        return Err(anyhow!(
            "Miri failed, or pinned nightly {nightly} / miri component missing \
             (rustup toolchain install {nightly} --component miri rust-src)"
        ));
    }
    Ok(())
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
    c.args([
        &format!("+{nightly}"),
        "test",
        "-Zbuild-std",
        &format!("--target={triple}"),
        "-p",
        krate,
        "--",
        "--test-threads=1",
    ]);
    c.env("RUSTFLAGS", "-Zsanitizer=thread");
    c.env("RUSTDOCFLAGS", "-Zsanitizer=thread");
    if !try_run(&format!("cargo +{nightly} test (TSAN)"), &mut c) {
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
    let mut c = at_root(cfg, "cargo");
    c.args(["test", "-p", krate, "--release"]);
    c.env("RUSTFLAGS", "--cfg loom");
    if !try_run("cargo test --release (loom)", &mut c) {
        return Err(anyhow!("loom failed"));
    }
    Ok(())
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

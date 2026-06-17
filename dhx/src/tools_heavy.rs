//! Heavy / nightly / container-bound tool wrappers (fuzz, miri, tsan, loom,
//! dst), split from `tools.rs` to keep each file within the ≤400 budget that
//! dhx enforces on itself (G4).
use crate::config::Config;
use crate::toolchain::{host_triple, pinned_nightly};
use crate::tools::at_root;
use crate::try_run;
use anyhow::{anyhow, Result};

pub(crate) fn fuzz(cfg: &Config, target: Option<String>, runs: u32) -> Result<()> {
    let target = target
        .or_else(|| cfg.raw.targets.fuzz.first().cloned())
        .ok_or_else(|| anyhow!("no fuzz target given and [targets].fuzz is empty"))?;
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

/// Resolve a required single-crate `[targets]` role, erroring if unset.
fn target_crate<'a>(role: &str, val: Option<&'a String>) -> Result<&'a str> {
    val.map(String::as_str)
        .ok_or_else(|| anyhow!("[targets].{role} is required for the `{role}` gate but is unset"))
}

pub(crate) fn miri(cfg: &Config) -> Result<()> {
    // `-Zmiri-disable-isolation` lets proptest read the clock / cwd it needs;
    // Miri still checks `todo-core` for UB. Tokio-runtime tests in the crate
    // are `#[cfg_attr(miri, ignore)]` (kqueue is unsupported under Miri).
    // Pinned nightly (nightly-version.txt): Miri's UB model evolves, so a
    // floating nightly would make this UB gate non-deterministic across days.
    let nightly = pinned_nightly(cfg)?;
    let krate = target_crate("miri", cfg.raw.targets.miri.as_ref())?;
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
    let nightly = pinned_nightly(cfg)?;
    let triple = host_triple();
    let krate = target_crate("tsan", cfg.raw.targets.tsan.as_ref())?;
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
    let krate = target_crate("loom", cfg.raw.targets.loom.as_ref())?;
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
    let dst = cfg
        .raw
        .targets
        .dst
        .as_ref()
        .ok_or_else(|| anyhow!("[targets].dst is required for the `dst` gate but is unset"))?;
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

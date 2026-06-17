//! Toolchain probes (host triple, pinned-nightly, version pins), split from
//! tools.rs. Shared by the tool wrappers and the TLC gate.
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::Command;

use crate::config::Config;

pub(crate) fn host_triple() -> String {
    // `rustc -vV` prints a `host: <triple>` line.
    let out = Command::new("rustc").arg("-vV").output();
    out.ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("host: ").map(ToOwned::to_owned))
        })
        .unwrap_or_else(|| "aarch64-apple-darwin".to_owned())
}

/// Read a single-line version-pin file (trimmed). These pins are what make the
/// `--full` external-tool gates deterministic across machines (Learning #20):
/// every floating binary is asserted against a tracked version before it runs.
pub(crate) fn read_pin(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read pin file {}", path.display()))?;
    Ok(raw.trim().to_owned())
}

/// The nightly toolchain spec pinned at `.harness/pins/nightly.txt`, e.g.
/// `nightly-2026-05-17`. Used by the deterministic nightly gates (miri, tsan)
/// so two machines on different days run the *same* nightly. Fuzz deliberately
/// does NOT use this — it is a discovery gate on floating nightly.
pub(crate) fn pinned_nightly(cfg: &Config) -> Result<String> {
    read_pin(cfg.path(".harness/pins/nightly.txt"))
}

/// Assert a tool's `--version`-style output contains the pinned token; return
/// an error (failing the gate) on mismatch. `probe` is the argv that prints
/// the version; `needle` is the pinned string that must appear in stdout+stderr.
pub(crate) fn assert_tool_version(label: &str, probe: &mut Command, needle: &str) -> Result<()> {
    let out = probe
        .output()
        .with_context(|| format!("{label}: could not run version probe"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    if text.contains(needle) {
        Ok(())
    } else {
        Err(anyhow!(
            "{label}: version pin mismatch — expected {needle:?} in output, got:\n{}",
            text.trim()
        ))
    }
}

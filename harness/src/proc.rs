//! Subprocess runners with per-gate timing, split from `main.rs` to keep it
//! within the ≤400-line budget dhx enforces on itself (G4).
use anyhow::{anyhow, Context, Result};
use std::process::Command;

/// Format an elapsed duration compactly (e.g. `0.4s`, `12s`, `3m04s`). A gate's
/// wall-clock cost is printed on every run so a cost *regression* (the silent
/// way a gate degrades toward not-completing — see the Kani-intractability
/// incident) is visible in the log instead of only surfacing as an eventual
/// stall.
fn fmt_elapsed(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs >= 60 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else if secs >= 10 {
        format!("{secs}s")
    } else {
        format!("{:.1}s", d.as_secs_f64())
    }
}

pub(crate) fn run(label: &str, cmd: &mut Command) -> Result<()> {
    println!("▶ {label}");
    let start = std::time::Instant::now();
    let status = cmd.status().with_context(|| format!("running {label}"))?;
    if !status.success() {
        return Err(anyhow!("{label} failed: {status}"));
    }
    println!("✓ {label} ({})", fmt_elapsed(start.elapsed()));
    Ok(())
}

pub(crate) fn try_run(label: &str, cmd: &mut Command) -> bool {
    println!("▶ {label}");
    let start = std::time::Instant::now();
    match cmd.status() {
        Ok(s) if s.success() => {
            println!("✓ {label} ({})", fmt_elapsed(start.elapsed()));
            true
        }
        Ok(s) => {
            eprintln!(
                "✗ {label} failed after {}: {s}",
                fmt_elapsed(start.elapsed())
            );
            false
        }
        Err(e) => {
            eprintln!("⚠ {label} skipped: {e}");
            false
        }
    }
}

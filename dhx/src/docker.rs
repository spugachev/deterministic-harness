//! Container orchestration (C1) + the single-source Dockerfile renderer (C12).
//!
//! `verify --full` runs in the pinned image so every external tool matches the
//! project's pins. Host `dhx verify --full` re-execs inside the image; inside,
//! the `DHX_IN_CONTAINER` sentinel short-circuits the re-exec.

use std::process::Command;

use anyhow::{anyhow, Context, Result};

use crate::config::Config;

const SENTINEL: &str = "DHX_IN_CONTAINER";

/// Are we already running inside the harness container?
pub(crate) fn in_container() -> bool {
    std::env::var(SENTINEL).is_ok_and(|v| v == "1")
}

/// Re-exec `dhx verify --full` inside the project's pinned image (C1). Source is
/// mounted read-only (write-mode gates run as `--check` in the container) and
/// `target/` is a named volume so container artifacts never poison the host
/// build. No Docker daemon ⇒ a hard error, never a host-tool fallback.
pub(crate) fn reexec_full(cfg: &Config) -> Result<()> {
    if Command::new("docker").arg("--version").output().is_err() {
        return Err(anyhow!(
            "`dhx verify --full` requires Docker (the gates run in the pinned image so tool \
             versions match). Install Docker, then build the image:\n  \
             docker build -t {img} .\nand re-run.",
            img = image_tag(cfg)
        ));
    }
    let img = image_tag(cfg);
    let target_vol = format!("{}-dhx-target", cfg.raw.project.name);
    let root = cfg.root.to_string_lossy().into_owned();
    println!("== verify --full → re-exec in container {img} ==");
    let mut c = Command::new("docker");
    c.args(["run", "--rm", "-e", &format!("{SENTINEL}=1")])
        .args(["-v", &format!("{root}:/work:ro")])
        .args(["-v", &format!("{target_vol}:/work/target")])
        .args(["-w", "/work", &img, "dhx", "verify", "--full"]);
    let status = c.status().context("docker run dhx verify --full")?;
    if !status.success() {
        return Err(anyhow!(
            "in-container `verify --full` failed ({status}). If the image is missing, build it: \
             docker build -t {img} ."
        ));
    }
    Ok(())
}

fn image_tag(cfg: &Config) -> String {
    format!("{}-harness:latest", cfg.raw.project.name)
}

/// Render the repo-root `Dockerfile` from the embedded template (C12). The
/// scaffold ships only `Dockerfile.template`; the concrete `Dockerfile` is
/// generated, and `--check` gates that it is up to date (drift = red, like
/// `regen`).
pub(crate) fn render_dockerfile(cfg: &Config, check: bool) -> Result<()> {
    let template = crate::init::scaffold_file("Dockerfile.template")
        .ok_or_else(|| anyhow!("embedded Dockerfile.template missing"))?;
    let rendered = template.replace("{{project}}", &cfg.raw.project.name);
    let out = cfg.path("Dockerfile");
    if check {
        let on_disk = std::fs::read_to_string(&out).unwrap_or_default();
        if on_disk != rendered {
            return Err(anyhow!(
                "Dockerfile drift: run `dhx render-dockerfile` and commit the result"
            ));
        }
        println!("✓ render-dockerfile --check: Dockerfile up to date");
    } else {
        std::fs::write(&out, rendered).with_context(|| format!("write {}", out.display()))?;
        println!("✓ render-dockerfile: wrote {}", out.display());
    }
    Ok(())
}

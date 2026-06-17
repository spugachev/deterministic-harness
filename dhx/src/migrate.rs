//! `dhx migrate` + `dhx pins update` — the upgrade seam (C6/F5).
//!
//! At schema v1 there is no prior schema to migrate FROM, so `migrate` is a
//! validated no-op: it confirms the config already speaks the current schema.
//! The command exists now so the *mechanism* ships before it is needed — when
//! schema 2 lands, the v1→v2 rewrite goes here and existing projects have an
//! actionable path instead of a hard `schema_version` error.

use anyhow::Result;

use crate::config::{Config, SCHEMA_VERSION};

pub(crate) fn run(cfg: &Config) -> Result<()> {
    // Reaching here means Config::load already validated schema_version, so the
    // file is current. (A mismatch would have errored in load with guidance.)
    println!(
        "✓ dhx migrate: harness.toml already at schema_version {SCHEMA_VERSION} (no migration needed at v1)"
    );
    let _ = cfg;
    Ok(())
}

/// Print the project's pinned tool versions. (A future version will support
/// bumping + flagging an image rebuild; v1 reports so a human can edit + rebuild.)
pub(crate) fn pins_update(cfg: &Config) -> Result<()> {
    println!("Pinned tool versions (edit the files in .harness/pins/, then rebuild the image):");
    for name in ["nightly", "verus", "tla2tools", "dhx"] {
        let path = cfg.path(&format!(".harness/pins/{name}.txt"));
        let val = std::fs::read_to_string(&path)
            .map_or_else(|_| "(absent)".to_owned(), |s| s.trim().to_owned());
        println!("  {name:<10} {val}");
    }
    println!(
        "\nAfter editing a pin, rebuild: docker build -t {}-harness:latest .",
        cfg.raw.project.name
    );
    Ok(())
}

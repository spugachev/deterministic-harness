//! Container guard. In this project there is no host `dhx` and no per-project
//! image: the single `dhx:latest` image (built from the repo-root `Dockerfile`)
//! bakes the `dhx` binary plus every pinned tool, and EVERY tier runs inside it
//! via `docker run … dhx <cmd>`. The image sets `DHX_IN_CONTAINER=1`, so dhx can
//! tell it is in the right environment and refuse to run gates anywhere else
//! (running against host tool versions would defeat determinism).

use anyhow::{anyhow, Result};

const SENTINEL: &str = "DHX_IN_CONTAINER";

/// Are we running inside the `dhx:latest` image?
pub(crate) fn in_container() -> bool {
    std::env::var(SENTINEL).is_ok_and(|v| v == "1")
}

/// Hard-fail unless we're inside the image. `verify` calls this so a gate never
/// runs against unpinned host tools (the silent-nondeterminism trap). The error
/// shows how to run the same command the supported way.
pub(crate) fn require_container(cmd: &str) -> Result<()> {
    if in_container() {
        return Ok(());
    }
    Err(anyhow!(
        "`dhx {cmd}` must run inside the dhx image so every tool matches the pins.\n\
         Define this shell function once (it mounts cache volumes so the second run\n\
         onward is fast — deps download once into a shared registry, target/ is a\n\
         per-project volume), then call `dhx {cmd}`:\n\n  \
         docker build -t dhx:latest .   # one time\n  \
         dhx() {{ docker run --rm -v \"$PWD\":/work -w /work \\\n           \
           -v dhx-cargo-registry:/root/.cargo/registry \\\n           \
           -v \"dhx-target-$(basename \"$PWD\")\":/work/target \\\n           \
           dhx:latest dhx \"$@\"; }}"
    ))
}

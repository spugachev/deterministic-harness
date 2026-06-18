---
name: check
description: Run the cheap deterministic gates (dhx check) and report failures
allowed-tools: Bash
---

# Cheap gate run

Every tier runs inside the `dhx:latest` image (there is no host `dhx`); the cache
volumes keep re-runs fast:

!`docker run --rm -v "$PWD":/work -w /work -v dhx-cargo-registry:/root/.cargo/registry -v "dhx-target-$(basename "$PWD")":/work/target dhx:latest dhx check 2>&1`

If any gate failed above, fix the root cause (not the symptom) and re-run.

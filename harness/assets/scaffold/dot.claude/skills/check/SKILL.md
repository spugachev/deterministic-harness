---
name: check
description: Run the cheap deterministic gates (dhx check) and report failures
allowed-tools: Bash
---

# Cheap gate run

Every tier runs inside the `dhx:latest` image (there is no host `dhx`):

!`docker run --rm -v "$PWD":/work -w /work dhx:latest dhx check 2>&1`

If any gate failed above, fix the root cause (not the symptom) and re-run.

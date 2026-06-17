# Configuration — `harness.toml` reference

`harness.toml` at the project root is the **one** manifest `dhx` reads. `dhx`
finds it by walking up from the cwd, so commands work from any subdirectory.
Resolution is two layers only: an explicit value in `harness.toml`, else a typed
default. Auto-discovery (the coverage ignore-set) only *suggests* — and prints
that it did. Inspect any gate's resolved value with:

```sh
dhx config explain coverage   # | targets | fsm | docs
```

## Full example

```toml
[meta]
schema_version = 1                    # REQUIRED. dhx refuses a mismatch with a
                                      # migrate hint rather than a confusing error.

[project]
name = "my-svc"                       # REQUIRED. Used e.g. for the temp-dir + image tag.

[coverage]
core = ["core"]                       # REQUIRED. The verified-core crate(s) held to
                                      # the high bar; every OTHER workspace member is
                                      # auto-excluded from the report (printed).
fail_under_lines = 90                 # default 90
fail_under_functions = 90             # default 90

[targets]                             # crate "roles" cargo metadata cannot infer
miri = "core"                         # which crate the UB check runs on
tsan = "core"                         # which crate the data-race check runs on
loom = "core"                         # which crate the interleaving check runs on
dst  = { crate = "api", test = "dst" }    # optional: the DST integration test
fuzz = ["validate_input"]             # optional: libFuzzer target names

[configs]                             # relocated tool configs (dhx passes the path flag)
deny     = ".harness/config/deny.toml"
gitleaks = ".harness/config/gitleaks.toml"
mutants  = ".harness/config/mutants.toml"
nextest  = ".harness/config/nextest.toml"

[docs]
requirements_dir  = "spec/requirements"
adr_dir           = "spec/adr"
features_dir      = "spec/features"   # where cucumber .feature files live
spec_dir          = "spec/tla"        # .tla/.cfg + mutations.toml
traceability_lock = "spec/traceability.lock.json"
readme            = "README.md"       # counts written between <!-- dhx:counts --> markers
req_id_pattern    = "REQ-\\d{3}"      # the id scheme is configurable
adr_id_pattern    = "ADR-\\d{4}"

[fsm]                                 # OPT-IN: enables regen + check-spec-sync
source          = "crates/core/src/domain/state.rs"
fn_name         = "next"
priority_source = "crates/core/src/domain/state.rs"
state_enum      = "TodoState"         # the parser is not wedded to any names
event_enum      = "Event"
generated_stem  = "Lifecycle"         # → spec/tla/Lifecycle.{tla,cfg}

[fsm.verus_dup]                       # OPT-IN within [fsm]: a Verus duplicate to keep in sync
file    = "crates/core/src/verus_proofs.rs"
spec_fn = "next_spec"
exec_fn = "next"

[verus]                               # OPT-IN: enables the verus gate
entry = "crates/core/src/verus_proofs.rs"
```

## Required vs optional

- **Required:** `[meta].schema_version`, `[project].name`, `[coverage].core`, and
  the single-crate `[targets]` entries for any role gate you run
  (`miri`/`tsan`/`loom`). Missing a required field is a **load error** that names
  it — not a silent skip. `[targets]`/`[coverage]` crate names are cross-checked
  against `cargo metadata`, so a typo fails at load, not mid-run.
- **Optional / opt-in:** `[fsm]`, `[fsm.verus_dup]`, `[verus]`, `[targets].dst`,
  `[targets].fuzz`. Absent ⇒ that gate prints a SKIPPED line — **unless the input
  exists on disk**, in which case it is mandatory (see "presence ⇒ mandatory" in
  [architecture.md](architecture.md)).

## Version pins (the single source of truth)

Pins live as one-line files under `.harness/pins/` (no `[pins]` table — the
paths are convention):

| File | Pins | Used by |
|---|---|---|
| `nightly.txt` | the nightly toolchain | miri, tsan |
| `verus.txt` | the Verus build | verus (asserted before proving) |
| `tla2tools.txt` | the TLC jar version | tlc (asserted against the banner) |
| `dhx.txt` | the dhx version | the Docker image (host ↔ container match) |

The `--full` Docker image is built **from** these files, so in-container tool
versions always match — there is no second source to drift. `dhx pins update`
prints the current pins and the rebuild command.

## The anti-vacuity table — `spec/tla/mutations.toml`

Every `INVARIANT`/`PROPERTY` declared in a `.cfg` must be either broken by a
mutation here (which TLC is *required* to catch) or justified as exempt:

```toml
[[mutations]]
spec    = "Lifecycle"
label   = "archived is not terminal"
find    = "(state = \"Archived\" => ~ ENABLED Next)"   # must occur exactly once
replace = "(state = \"Archived\" => ENABLED Next)"
expect  = "ArchivedTerminal"                            # the invariant TLC must report

[[exempt]]
spec   = "Lifecycle"
name   = "TypeInvariant"
reason = "structural well-typedness catch-all; ArchivedTerminal carries the substantive claim"
```

## Upgrading dhx

`harness.toml` carries `schema_version`. A newer `dhx` reading an older config
emits an actionable message (run `dhx migrate`) rather than a bare failure. At
schema 1 `dhx migrate` is a validated no-op — the seam exists so future upgrades
have a path.

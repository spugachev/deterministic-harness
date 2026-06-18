# CLAUDE.md — conventions for developing `dhx` itself

This repo is the **Deterministic Harness** system. `dhx` (the CLI binary, whose
crate lives in `harness/`) scaffolds + verifies _other_ projects;
`harness/assets/scaffold/` is the embedded template it writes. This file governs
working on **dhx itself** — it is NOT the file a scaffolded project gets (that is
`harness/assets/scaffold/CLAUDE.md.template`). Read [README.md](README.md) for
the approach, the philosophy, and the per-tool detail.

> **The crate folder is `harness/`; the binary/command is `dhx`.** The directory
> was renamed `dhx/` → `harness/`; the crate name and published CLI stay `dhx`
> (so `cargo build -p dhx`, `cargo install --path harness`).

## Generated trees — never hand-edit

`examples/` is **generated** — each example is produced by `dhx init` + an agent
run, then verified. Do NOT edit files under `examples/` directly; regenerate the
example instead. The source of truth for anything that looks scaffolded is
`harness/assets/scaffold/`, never a copy materialized from it.

## Invariants — applied to every change

1. **English only** — code, comments, identifiers, docs, commit messages.
2. **`dhx` holds itself to its own rules (dogfood / G4).** Before committing,
   the universal subset of dhx's own gates must pass on dhx's source:
   - `cargo fmt --all --check`
   - `cargo clippy --workspace --all-targets` clean (the lints below)
   - `cargo test -p dhx` green
   - **every `harness/src/*.rs` file ≤ 400 lines** — the limit dhx enforces on
     others. If a file would exceed it, split it (e.g. `tools.rs` →
     `tools_heavy.rs`, `main.rs` → `proc.rs`, `config.rs` → `config_explain.rs`,
     `fsm.rs` → `fsm_render.rs`). This is non-negotiable.
     An unverified verifier is the ultimate toothless gate.
3. **Only verification techniques are gates.** Persistence/HTTP/app concerns
   (sqlx, an OpenAPI contract test, …) are NOT harness gates and do not belong in
   `dhx`. The harness requires the _ports architecture_, not any specific library.

> **Note — dhx is the tool, not a scaffolded service.** dhx has no
> `harness.toml`, no FSM/ports of its own: those belong to the _projects dhx
> scaffolds_. `dhx verify` is exercised inside a scaffolded project (and by the
> `shipped_scaffold_*` self-tests that materialize the embedded scaffold and
> validate it), not against this repo. dhx's own CI-equivalent is the self-verify
> subset in invariant 2.

> **One image, no host dhx.** The repo root holds a single hand-written
> `Dockerfile` that builds `dhx:latest` — the `dhx` binary plus every pinned
> tool. There is no `dhx` on the host and no per-project Dockerfile: every tier
> (`init`, `check`, `verify`) runs via `docker run … dhx:latest dhx <cmd>`. After
> editing the `Dockerfile` or a scaffold pin, rebuild it.

## The approach dhx scaffolds (what the gates enforce in every project)

dhx exists to make one workflow reliable in the projects it scaffolds —
**specification → code → simulation**, never test-first TDD:

1. **Spec first.** A `REQ-NNN.md` (EARS acceptance criteria), then the executable
   spec: **BDD+EARS Gherkin scenarios always**, plus **TLA+** when the feature is
   concurrent / a protocol (the lifecycle FSM's TLA+ is generated from the Rust by
   `dhx regen`). Every TLA+ invariant carries an anti-vacuity mutation.
2. **Code** derived from the spec — pure logic in the IO-free core, IO behind
   ports in outer crates.
3. **Simulation + proof** — unit + property (proptest) tests, coverage proven
   non-vacuous by mutation testing, then DST/Kani/Loom/TSAN/fuzz routed by the
   feature's hardest question.

**The mandatory floor on every feature: BDD+EARS (a scenario per REQ, no opt-out),
clippy, and property tests.** Everything else is routed by need — running every
tool on every feature is ceremony. `check-bdd-coverage` fails a REQ with no
scenario; a `(verified=…)` marker supplements a scenario, never replaces it.

**Tiers are split by wall-clock so verification is continuous, not pre-release:**
- `dhx check` — every save (~s): fmt + clippy + all meta-gates.
- `dhx verify --quick` — after small changes / each commit (~1-2 min): unit +
  proptest + coverage + Kani + **TLA+/TLC + its mutation** (spec checked as early
  as code) + deny/gitleaks/machete + 1 DST seed.
- `dhx verify --full` — after big changes / before release: adds only the
  expensive instruments — TSAN, mutants, fuzz, Loom, multi-seed DST. (Memory UB
  is handled at compile time by `#![forbid(unsafe_code)]`, so there is no Miri
  gate; reintroduce one per-crate only if a crate lifts the `unsafe` ban.)

This is encoded for the scaffolded project in
`harness/assets/scaffold/CLAUDE.md.template` (and `verify_quick`/`verify_full` in
`main.rs`). Keep the two in sync when the tiering changes.

## Lint discipline

`harness/Cargo.toml` `[lints]` is the source of truth: `clippy::all + pedantic`
denied, `warnings = "deny"`, `unsafe_op_in_unsafe_fn = "forbid"`. A developer-CLI
allows `print_*`/`unwrap`/`expect`/`panic` (it's a tool, not a library). The only
escape from a deny is a site-level `#[allow(lint, reason = "…")]` that **must
carry a reason** — a bare `#[allow]` is not acceptable.

## Layout

```
harness/
├── Cargo.toml              [[bin]] dhx; include = ["src/**","assets/**"]
├── assets/scaffold/        the embedded template (include_dir!) — inert names:
│                           dot.* and *.tmpl, renamed on `dhx init`
└── src/
    ├── main.rs             clap Cmd enum + dispatch + the verify tiers + preflight table
    ├── proc.rs             run / try_run (subprocess + per-gate timing)
    ├── config.rs           harness.toml → validated Config (schema, required, shape)
    ├── config_explain.rs   `dhx config explain`
    ├── corpus.rs           the SINGLE requirements()/scenarios()/tla_specs() walk
    ├── init.rs             `dhx init` scaffolder (dot/.tmpl rename, git, regen, stamp)
    ├── migrate.rs          schema-version seam + `dhx pins update`
    ├── docker.rs           the container guard (require_container / in_container)
    ├── fsm.rs / fsm_render  FSM extraction (syn) + TLA rendering
    ├── fsm_sync.rs          check-spec-sync
    ├── traceability.rs      check-traceability (+ lock --check/--write)
    ├── bdd_style.rs / bdd_gates.rs   the BDD + verified-marker gates
    ├── docs_gates.rs        check-file-size + check-docs-counts
    ├── tlc.rs               tlc / tlc --mutate / check-mutation-coverage
    ├── toolchain.rs         pins + version assertion + host triple
    ├── tools.rs / tools_heavy.rs   the external-tool wrappers
    └── tests.rs             gate self-tests (parsers/tables) — `cargo test -p dhx`
```

## Conventions for new code

- **Every gate takes `&Config` and runs at `cfg.root`** (`tools::at_root`) so dhx
  works from any subdirectory. No hardcoded project paths — everything comes from
  `Config`.
- **No duplicated filesystem walks.** REQ / scenario / `.tla` discovery lives
  once in `corpus.rs`; a new gate consumes it, never re-walks (drift between two
  walks is a silent-toothlessness vector).
- **Presence ⇒ mandatory.** A new opt-in gate must FAIL when its input exists on
  disk but is unconfigured — never silently skip. Mirror the existing pattern in
  `config.rs::validate_fsm_shape` / `tlc::check_mutation_coverage`.
- **Embedded assets are inert.** Anything under `assets/scaffold/` is data, never
  compiled: nested manifests are `Cargo.toml.tmpl`, dotfiles are `dot.*`. They
  must survive `cargo package` / `cargo install` (verify with the install
  round-trip).
- **New subcommands** go in the `Cmd` enum + `main()` dispatch; thread `&cfg`.

## Verifying a change end-to-end

```sh
cargo build -p dhx && cargo test -p dhx          # builds + self-tests
cargo fmt --all --check && cargo clippy --workspace --all-targets
cargo install --path harness --force              # embeds current assets
dhx init /tmp/probe && cd /tmp/probe && dhx check # scaffold → 11 gates green
```

If you change `assets/scaffold/`, always re-run `cargo install --path harness
--force` before testing `dhx init` — the assets are embedded at build time.

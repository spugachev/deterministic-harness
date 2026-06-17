# CLAUDE.md — conventions for developing `dhx` itself

This repo is the **Deterministic Harness** system. `dhx` (in `dhx/`) is the CLI
that scaffolds + verifies *other* projects; `dhx/assets/scaffold/` is the
embedded template it writes. This file governs working on **dhx itself** — it is
NOT the file a scaffolded project gets (that is
`dhx/assets/scaffold/CLAUDE.md.template`). Read [docs/](docs/) for the design.

## Invariants — applied to every change

1. **English only** — code, comments, identifiers, docs, commit messages.
2. **`dhx` holds itself to its own rules (dogfood / G4).** Before committing,
   the universal subset of dhx's own gates must pass on dhx's source:
   - `cargo fmt --all --check`
   - `cargo clippy --workspace --all-targets` clean (the lints below)
   - `cargo test -p dhx` green
   - **every `dhx/src/*.rs` file ≤ 400 lines** — the limit dhx enforces on
     others. If a file would exceed it, split it (e.g. `tools.rs` →
     `tools_heavy.rs`, `main.rs` → `proc.rs`, `config.rs` → `config_explain.rs`,
     `fsm.rs` → `fsm_render.rs`). This is non-negotiable.
   An unverified verifier is the ultimate toothless gate.
3. **Only verification techniques are gates.** Persistence/HTTP/app concerns
   (sqlx, an OpenAPI contract test, …) are NOT harness gates and do not belong in
   `dhx`. The harness requires the *ports architecture*, not any specific library.

## Lint discipline

`dhx/Cargo.toml` `[lints]` is the source of truth: `clippy::all + pedantic`
denied, `warnings = "deny"`, `unsafe_op_in_unsafe_fn = "forbid"`. A developer-CLI
allows `print_*`/`unwrap`/`expect`/`panic` (it's a tool, not a library). The only
escape from a deny is a site-level `#[allow(lint, reason = "…")]` that **must
carry a reason** — a bare `#[allow]` is not acceptable.

## Layout

```
dhx/
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
    ├── docker.rs           --full container re-exec + `render-dockerfile`
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
cargo build -p dhx && cargo test -p dhx          # builds + 6 self-tests
cargo fmt --all --check && cargo clippy --workspace --all-targets
cargo install --path dhx --force                  # embeds current assets
dhx init /tmp/probe && cd /tmp/probe && dhx check # scaffold → 11 gates green
```

If you change `assets/scaffold/`, always re-run `cargo install --path dhx
--force` before testing `dhx init` — the assets are embedded at build time.

## Git workflow

Conventional-commit prefixes (`feat`/`fix`/`test`/`refactor`/`docs`/`chore`/…).
Work on `main`. **Never** `git --no-verify`. Do not bypass Code Defender or any
publication control; external pushes go through the sanctioned approval path.

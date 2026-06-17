# Deterministic Harness — manual

`dhx` scaffolds a new Rust service already wired into a comprehensive
verification toolchain, and runs every gate locally (there is no CI). This
manual explains *why* the harness is shaped the way it is, *what* each tool
does and is worth, and *how* to work inside it.

Read in order, or jump to what you need:

| # | Doc | What it covers |
|---|-----|----------------|
| 1 | [philosophy.md](philosophy.md) | Why a deterministic harness — oracle trust vs compiler trust, harness-first development, gates with teeth. The intellectual case. |
| 2 | [architecture.md](architecture.md) | The shape a scaffolded project must have: the IO-free verified core, the Clock/Rng/IdGen ports, the `spec/`+`.harness/` layout, and *why* that shape is what gives the gates teeth. |
| 3 | [workflow.md](workflow.md) | The methodology: the spec-first phase order (REQ → TLA+ → BDD → implement → cov/mutants → Kani/Verus → proptest → DST/Loom/TSAN/fuzz), the routing rubric, the three tiers, and the hard rules. |
| 4 | [toolchain.md](toolchain.md) | Every tool, one by one: what it does, how to run it, the bug class it closes, and a usefulness rating measured from data. The reference. |
| 5 | [configuration.md](configuration.md) | `harness.toml` reference — every key, its default, and how `dhx` resolves it. Version pins, tool-config relocation, opt-in gates. |

## The 30-second version

- **Agents (and humans) write code faster than it can be reviewed; "compiles +
  tests pass" is not "correct".** The lever is not a smarter model — it is a
  *tighter, deterministic harness* that verifies every edit. See
  [philosophy.md](philosophy.md).
- **The harness only has teeth because the project has a shape:** an IO-free core
  behind ports, so Kani/Verus/proptest can prove functions total and DST/Loom/
  TSAN can be deterministic. `dhx init` creates that shape. See
  [architecture.md](architecture.md).
- **You route tools to the feature's hardest question** — you do not run all of
  them on everything. See [workflow.md](workflow.md) and
  [toolchain.md](toolchain.md).
- **Three tiers:** `dhx check` (cheap, every edit) → `dhx verify --quick`
  (pre-push) → `dhx verify --full` (pre-release, in the pinned Docker image).

//! Self-tests for the gate machinery (parsers/tables), split from main.rs.
//! The gates check the *product*; these check the *gates' own parsers/tables*,
//! so a regression in the harness itself (a broken token extractor, a duplicate
//! mutation) is caught here. `cargo test -p xtask` runs them in the test gate.
#![allow(
    clippy::missing_docs_in_private_items,
    clippy::unwrap_used,
    reason = "test-only"
)]

use crate::bdd_gates::salient_tokens;
use crate::config::Config;
use crate::fsm::Transition;
use crate::fsm_sync::canonical_fsm;
use crate::tlc::MutationTable;
use crate::traceability::symbol_is_defined;

// --- salient_tokens: the HTTP-status anchor for bdd-coverage -------------

#[test]
fn salient_tokens_extracts_status_codes() {
    assert_eq!(salient_tokens("shall respond 409 on conflict"), vec!["409"]);
    // Multiple codes, sorted + deduped.
    assert_eq!(
        salient_tokens("201 on create, 200 on replay, 201 again"),
        vec!["200", "201"]
    );
    // 429 must be recognized (added for the rate-limit feature).
    assert_eq!(salient_tokens("shall respond 429"), vec!["429"]);
}

#[test]
fn salient_tokens_ignores_non_status_numbers() {
    // A bare prose number that is not an HTTP status yields nothing — so
    // bdd-coverage won't try to anchor on it (and won't pass vacuously).
    assert!(salient_tokens("at most 16 tags").is_empty());
    assert!(salient_tokens("trim leading whitespace").is_empty());
}

// --- symbol_is_defined: the rename-protection behind traceability --------

#[test]
fn symbol_is_defined_requires_a_real_definition() {
    let rs = "pub fn classify() {}\n// a comment mentioning ghost_fn\n";
    assert!(symbol_is_defined("x.rs", "classify", rs), "fn def found");
    // A mere mention in a comment must NOT count (the stale-rename trap).
    assert!(
        !symbol_is_defined("x.rs", "ghost_fn", rs),
        "comment mention must not satisfy the link"
    );
}

#[test]
fn symbol_is_defined_matches_tla_operators() {
    let tla = "RateGrant ==\n    /\\ tokens > 0\n";
    assert!(symbol_is_defined("Spec.tla", "RateGrant", tla));
    assert!(!symbol_is_defined("Spec.tla", "NotThere", tla));
}

// --- canonical_fsm: the order-independent FSM compare for spec-sync ------

#[test]
fn canonical_fsm_is_order_independent() {
    let t1 = vec![
        Transition {
            source: "A".into(),
            event: "go".into(),
            target: "B".into(),
        },
        Transition {
            source: "B".into(),
            event: "stop".into(),
            target: "C".into(),
        },
    ];
    let t2 = vec![
        Transition {
            source: "B".into(),
            event: "stop".into(),
            target: "C".into(),
        },
        Transition {
            source: "A".into(),
            event: "go".into(),
            target: "B".into(),
        },
    ];
    // Same transitions in a different order canonicalize identically.
    assert_eq!(
        canonical_fsm(&["B".into(), "A".into()], &["go".into()], &t1),
        canonical_fsm(&["A".into(), "B".into()], &["go".into()], &t2)
    );
    // A genuinely different transition set must differ.
    let t3 = vec![Transition {
        source: "A".into(),
        event: "go".into(),
        target: "C".into(),
    }];
    assert_ne!(
        canonical_fsm(&["A".into()], &["go".into()], &t1),
        canonical_fsm(&["A".into()], &["go".into()], &t3)
    );
}

// --- mutation table integrity (the anti-vacuity meta-gate's data) -------
// The table is now a project data file (spec/tla/mutations.toml); these tests
// pin the PARSER + the well-formedness checks the gate relies on, using a
// representative in-memory document.

const SAMPLE_MUTATIONS: &str = r#"
[[mutations]]
spec = "ConcurrentApi"
label = "version may decrease"
find = "db'[id].version >= db[id].version"
replace = "db'[id].version < db[id].version"
expect = "VersionMonotone"

[[exempt]]
spec = "ConcurrentApi"
name = "TypeOK"
reason = "structural well-typedness catch-all"
"#;

#[test]
fn mutation_table_parses_and_is_well_formed() {
    let table: MutationTable = toml::from_str(SAMPLE_MUTATIONS).expect("parse mutations.toml");
    assert_eq!(table.mutations.len(), 1);
    assert_eq!(table.exempt.len(), 1);

    // No duplicate (spec, expect); find/replace differ and nonempty.
    let mut seen = std::collections::HashSet::new();
    for m in &table.mutations {
        assert!(!m.find.is_empty(), "{}: empty find", m.label);
        assert_ne!(m.find, m.replace, "{}: find == replace is a no-op", m.label);
        assert!(
            seen.insert((m.spec.clone(), m.expect.clone())),
            "duplicate mutation for {}/{}",
            m.spec,
            m.expect
        );
    }
    // A mutation and an exemption must not both claim the same invariant.
    let mutated: std::collections::HashSet<(&str, &str)> = table
        .mutations
        .iter()
        .map(|m| (m.spec.as_str(), m.expect.as_str()))
        .collect();
    for e in &table.exempt {
        assert!(!e.reason.is_empty(), "{}/{}: empty reason", e.spec, e.name);
        assert!(
            !mutated.contains(&(e.spec.as_str(), e.name.as_str())),
            "{}/{} is both mutated and exempted — pick one",
            e.spec,
            e.name
        );
    }
}

// --- scaffold integrity (C-EX): the SHIPPED scaffold must load + validate -----
// dhx materializes its own embedded scaffold to a temp dir and asserts the
// shipped harness.toml is structurally valid (schema, required fields, FSM
// shape, crate-name cross-check against the shipped Cargo.toml). A broken
// scaffold therefore fails dhx's build, not just a manual `dhx init`.

fn unique_tmp(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let pid = std::process::id();
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("dhx-scaffold-test-{tag}-{pid}-{n}"))
}

#[test]
fn shipped_scaffold_loads_and_validates() {
    let dir = unique_tmp("ok");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    crate::init::materialize_to(&dir, "probe").expect("materialize embedded scaffold");

    // The renamed harness.toml must exist (proves dot/.tmpl rename + substitution).
    assert!(
        dir.join("harness.toml").is_file(),
        "harness.toml materialized"
    );
    assert!(
        dir.join(".harness/pins/nightly.txt").is_file(),
        "dotfiles renamed"
    );
    assert!(
        dir.join("crates/core/Cargo.toml").is_file(),
        "manifest un-tmpl'd"
    );

    // Config::load_from runs the full validate() — schema, required fields, FSM
    // presence-mandatory, AND the C14 crate-name cross-check against metadata.
    Config::load_from(&dir).expect("shipped scaffold harness.toml is valid");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn coverage_typo_is_a_load_error() {
    // C14 teeth: a non-existent crate in [coverage].core fails at load.
    let dir = unique_tmp("typo");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    crate::init::materialize_to(&dir, "probe").unwrap();
    let manifest = dir.join("harness.toml");
    let bad = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("core = [\"core\"]", "core = [\"c0re\"]");
    std::fs::write(&manifest, bad).unwrap();

    let err = Config::load_from(&dir).expect_err("typo'd core crate must fail load");
    assert!(
        err.to_string().contains("c0re") && err.to_string().contains("not a workspace member"),
        "error should name the typo: {err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

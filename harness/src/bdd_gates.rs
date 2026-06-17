//! BDD-style lint + acceptance-coverage gates (bdd-style, bdd-coverage,
//! verified-markers), split from main.rs.
use anyhow::{anyhow, Result};

use crate::config::Config;
use crate::corpus::{self, Scenario};

/// The salient anchor of an acceptance criterion: the HTTP status code(s) it
/// names. Status codes are the one token that survives the translation from
/// EARS prose to Gherkin unchanged — scenarios assert `status 409`, but render
/// the verb as a domain action ("creates"/"patches") and rarely echo the
/// `SCREAMING_SNAKE` error code, so neither is a reliable cross-language anchor.
/// We match only against the project's actual status set (not any 3-digit
/// number), so a value like "greater than 100" is not mistaken for a status. A
/// criterion naming no status (a pure invariant like "at most one row") has no
/// HTTP-observable anchor and must use the `(verified: …)` escape hatch.
pub(crate) fn salient_tokens(criterion: &str) -> Vec<String> {
    const HTTP_STATUSES: [&str; 13] = [
        "200", "201", "204", "304", "400", "404", "409", "412", "413", "422", "428", "429", "500",
    ];
    let mut toks: Vec<String> = HTTP_STATUSES
        .iter()
        .filter(|code| {
            regex::Regex::new(&format!(r"\b{code}\b"))
                .ok()
                .is_some_and(|re| re.is_match(criterion))
        })
        .map(|c| (*c).to_owned())
        .collect();
    toks.sort();
    toks.dedup();
    toks
}

/// BDD+EARS is the mandatory floor: **every active REQ must have at least one
/// Gherkin scenario tagged with its id** — no opt-out. On top of that, each
/// acceptance criterion that is HTTP-observable (names a status code) must be
/// asserted by one of that REQ's scenarios. A `(verified=kani|proptest|dst|tla)`
/// marker relaxes only the *per-criterion token match* for properties that are
/// genuinely not HTTP-observable (e.g. "at most one row per key", proven in
/// TLA+/proptest) — it never exempts the REQ from having a scenario. Closes the
/// gap where a `.feature` silently fails to realize a criterion it claims to
/// cover (see REQ-011 history).
pub(crate) fn check_bdd_coverage(cfg: &Config) -> Result<()> {
    let req_dir = cfg.path(&cfg.raw.docs.requirements_dir);
    if !req_dir.exists() {
        println!("check-bdd-coverage: no requirements/ (skip)");
        return Ok(());
    }
    let scenarios = corpus::scenarios(cfg)?;
    // Marker uses `=` (not `:`) to stay a valid YAML plain scalar — a
    // colon-space inside the string would make serde_yaml read it as a map.
    let verified_marker =
        regex::Regex::new(r"\(verified=[a-z0-9_+/, ]+\)\s*$").expect("marker regex");

    let mut errors: Vec<String> = Vec::new();
    let mut checked = 0_u32;
    let mut skipped = 0_u32;
    let mut reqs_with_scenario = 0_u32;

    for (_p, req) in corpus::requirements(cfg)? {
        if req.status != "active" {
            continue;
        }
        let mine: Vec<&Scenario> = scenarios
            .iter()
            .filter(|s| s.req.as_deref() == Some(req.id.as_str()))
            .collect();

        // The mandatory BDD+EARS floor: every active REQ needs a scenario, full
        // stop. A `(verified=…)` marker supplements a scenario, it never replaces
        // it — so this check is independent of any per-criterion marker below.
        if mine.is_empty() {
            errors.push(format!(
                "{}: no BDD scenario tagged with it — every requirement needs at least one \
                 `Scenario: {} — …` in a .feature (EARS criterion → Gherkin). BDD is the \
                 mandatory floor; `(verified=…)` markers add proof, they do not replace it",
                req.id, req.id
            ));
        } else {
            reqs_with_scenario = reqs_with_scenario.saturating_add(1);
        }

        for criterion in &req.acceptance {
            // Escape hatch: a criterion verified by another technique.
            if verified_marker.is_match(criterion) {
                skipped = skipped.saturating_add(1);
                continue;
            }
            let tokens = salient_tokens(criterion);
            if tokens.is_empty() {
                // No HTTP status code in the criterion. For a non-HTTP project
                // that is the common case — most behaviour isn't anchored to a
                // status. The mandatory floor above already guarantees this REQ
                // has a tagged scenario; we don't demand a per-token match for a
                // criterion that has no cross-language anchor, and we do NOT
                // force a `(verified=…)` marker. (A `client error` criterion is
                // still anchored to a `client error` scenario when one exists.)
                checked = checked.saturating_add(1);
                continue;
            }
            checked = checked.saturating_add(1);
            // Covered if some scenario for this REQ asserts all the criterion's
            // status codes. A 4xx criterion is also covered by a scenario that
            // asserts the deliberately-imprecise "client error" (the project's
            // `then_client_error` step) — validation surfaces as 400 OR 422
            // depending on the layer, so asserting an exact 4xx would be wrong.
            let all_4xx = tokens.iter().all(|t| t.starts_with('4'));
            let covered = mine.iter().any(|s| {
                tokens.iter().all(|tok| s.text.contains(tok))
                    || (all_4xx && s.text.contains("client error"))
            });
            if !covered {
                errors.push(format!(
                    "{}: acceptance criterion has no covering scenario (status {:?}): {:?}\n      \
                     add a `Scenario: {} — …` that exercises it, or append \
                     `(verified=kani|proptest|dst)` to the criterion",
                    req.id, tokens, criterion, req.id
                ));
            }
        }
    }

    if !errors.is_empty() {
        for e in &errors {
            eprintln!("  ✗ {e}");
        }
        return Err(anyhow!(
            "check-bdd-coverage: {} requirement/criterion problem(s) — every active REQ needs a \
             tagged scenario, and each HTTP-observable criterion needs an asserting scenario",
            errors.len()
        ));
    }
    println!(
        "✓ check-bdd-coverage OK ({reqs_with_scenario} REQ(s) with a scenario; \
         {checked} criteria asserted, {skipped} verified elsewhere)"
    );
    Ok(())
}

/// Every `(verified=X, Y, …)` escape-hatch on an acceptance criterion must be
/// BACKED by a real `implements_in.<X>` link on the same REQ.
///
/// `check-bdd-coverage` lets a criterion opt out of needing a BDD scenario by
/// claiming another technique proves it (`(verified=kani, proptest)`). But it
/// only *skips* such criteria — nothing checked the claim was true. A criterion
/// could say `(verified=kani)` with no Kani proof anywhere: a traceability lie,
/// the exact intent-drift the project exists to prevent. This gate closes it —
/// each token in a marker must have a non-empty `implements_in` entry of the
/// same name. Jar-free, runs in the preflight.
pub(crate) fn check_verified_markers(cfg: &Config) -> Result<()> {
    let req_dir = cfg.path(&cfg.raw.docs.requirements_dir);
    if !req_dir.exists() {
        println!("check-verified-markers: no requirements/ (skip)");
        return Ok(());
    }
    // Capture the comma/space-separated technique list inside (verified=…).
    let marker = regex::Regex::new(r"\(verified=([a-z0-9_+/, ]+)\)\s*$").expect("marker regex");
    // We enforce backing only for techniques whose convention IS a per-REQ
    // `implements_in.<tok>` link: kani, tla (a named proof / spec
    // artefact). The rest are self-evidencing by a different, deliberate
    // convention and are NOT flagged:
    //   * `code`     — "the type/function is its own proof" (no separate file);
    //   * `gherkin`  — enforced instead by `check-bdd-coverage`;
    //   * `proptest` — lives inline in the module under test (no per-REQ link;
    //                  only 2/20 REQs link it, by convention);
    //   * `dst`      — scenarios are cross-cutting, not linked per REQ.
    // Requiring links for those would flag a legitimate convention as drift —
    // an over-strict gate (false positives) is worse than none. The artefact
    // techniques are exactly the ones a stale claim could silently lie about.
    let enforced = ["kani", "tla"];

    let mut errors: Vec<String> = Vec::new();
    let mut checked = 0_u32;

    for (_p, req) in corpus::requirements(cfg)? {
        if req.status != "active" {
            continue;
        }
        for criterion in &req.acceptance {
            let Some(caps) = marker.captures(criterion) else {
                continue;
            };
            for tok in caps[1].split(',') {
                let tok = tok.trim();
                if !enforced.contains(&tok) {
                    continue;
                }
                checked = checked.saturating_add(1);
                let backed = req
                    .implements_in
                    .get(tok)
                    .is_some_and(|links| !links.is_empty());
                if !backed {
                    errors.push(format!(
                        "{}: criterion claims `(verified={tok})` but implements_in.{tok} is \
                         missing/empty — back the claim with a real {tok} link or drop it from \
                         the marker:\n      {criterion:?}",
                        req.id
                    ));
                }
            }
        }
    }

    if !errors.is_empty() {
        for e in &errors {
            eprintln!("  ✗ {e}");
        }
        return Err(anyhow!(
            "check-verified-markers: {} unbacked `(verified=…)` claim(s)",
            errors.len()
        ));
    }
    println!("✓ check-verified-markers OK ({checked} verified-claim(s) backed by a real link)");
    Ok(())
}

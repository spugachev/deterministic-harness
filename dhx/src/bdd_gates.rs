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

/// Every active REQ's acceptance criterion must be covered by at least one BDD
/// scenario tagged with that REQ id — unless the criterion opts out with a
/// trailing `(verified: kani|proptest|dst|...)` marker for properties that are
/// genuinely not HTTP-observable (e.g. "at most one row per key", proven in
/// TLA+/proptest). Closes the gap where a `.feature` silently fails to realize
/// a criterion it claims to cover (see REQ-011 history).
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

    for (_p, req) in corpus::requirements(cfg)? {
        // Only REQs that declare a gherkin link are held to BDD coverage.
        if req.status != "active" || !req.implements_in.contains_key("gherkin") {
            continue;
        }
        let mine: Vec<&Scenario> = scenarios
            .iter()
            .filter(|s| s.req.as_deref() == Some(req.id.as_str()))
            .collect();

        for criterion in &req.acceptance {
            // Escape hatch: a criterion verified by another technique.
            if verified_marker.is_match(criterion) {
                skipped = skipped.saturating_add(1);
                continue;
            }
            let tokens = salient_tokens(criterion);
            if tokens.is_empty() {
                // A criterion phrased as the imprecise "client error" (no exact
                // status, because the layer decides 400-vs-422) is anchorable by
                // a "client error" scenario for the same REQ.
                let crit_lc = criterion.to_lowercase();
                if crit_lc.contains("client error")
                    && mine.iter().any(|s| s.text.contains("client error"))
                {
                    checked = checked.saturating_add(1);
                    continue;
                }
                // Otherwise: no reliable cross-language anchor. We do NOT pass
                // it vacuously — it must opt out explicitly via `(verified=…)`.
                errors.push(format!(
                    "{}: acceptance criterion has no HTTP status code to anchor a \
                     scenario match: {:?}\n      if it is HTTP-observable, phrase \
                     it with its status code (or 'client error'); otherwise append \
                     `(verified=kani|proptest|dst|tla)`",
                    req.id, criterion
                ));
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
            "check-bdd-coverage: {} acceptance criterion(s) without a BDD scenario",
            errors.len()
        ));
    }
    println!(
        "✓ check-bdd-coverage OK ({checked} criteria covered by scenarios, \
         {skipped} verified elsewhere)"
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
    // `implements_in.<tok>` link: kani, verus, tla (a named proof / spec
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
    let enforced = ["kani", "verus", "tla"];

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

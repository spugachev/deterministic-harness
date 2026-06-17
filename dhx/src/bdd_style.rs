//! Gherkin/EARS style lint (check-bdd-style), split from `bdd_gates.rs`.
use anyhow::{anyhow, Result};

use crate::config::Config;

pub(crate) fn check_bdd_style(cfg: &Config) -> Result<()> {
    let dir = cfg.path(&cfg.raw.docs.features_dir);
    if !dir.exists() {
        println!("check-bdd-style: no features/ yet (skip)");
        return Ok(());
    }

    let mut files = 0_u32;
    let mut scenarios = 0_u32;
    let mut errors: Vec<String> = Vec::new();

    for entry in walkdir::WalkDir::new(&dir) {
        let path = entry?.path().to_path_buf();
        if path.extension().and_then(|x| x.to_str()) != Some("feature") {
            continue;
        }
        files = files.saturating_add(1);
        let raw = std::fs::read_to_string(&path)?;
        scenarios = scenarios.saturating_add(lint_feature_file(&path, &raw, &mut errors));
    }

    if !errors.is_empty() {
        for err in &errors {
            eprintln!("  ✗ {err}");
        }
        return Err(anyhow!(
            "check-bdd-style: {} EARS/Gherkin violation(s) across {files} file(s)",
            errors.len()
        ));
    }

    println!("✓ check-bdd-style OK ({files} feature file(s), {scenarios} scenario(s))");
    Ok(())
}

/// Lint one `.feature` file, pushing any violations onto `errors`. Returns the
/// number of `Scenario:` declarations seen (for the summary count).
fn lint_feature_file(path: &std::path::Path, raw: &str, errors: &mut Vec<String>) -> u32 {
    let where_ = |n: usize| format!("{}:{}", path.display(), n);

    let mut scenarios = 0_u32;
    let mut saw_feature = false;
    let mut in_scenario = false;
    let mut steps_in_scenario = 0_u32;
    let mut last_keyword = "";

    for (idx, line) in raw.lines().enumerate() {
        let n = idx.saturating_add(1);
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }

        if let Some(rest) = t.strip_prefix("Feature:") {
            saw_feature = true;
            if rest.trim().is_empty() {
                errors.push(format!("{}: Feature has no title", where_(n)));
            }
            continue;
        }

        if let Some(rest) = t
            .strip_prefix("Scenario:")
            .or_else(|| t.strip_prefix("Scenario Outline:"))
        {
            // close previous scenario
            if in_scenario && steps_in_scenario == 0 {
                errors.push(format!("{}: previous scenario has no steps", where_(n)));
            }
            scenarios = scenarios.saturating_add(1);
            in_scenario = true;
            steps_in_scenario = 0;
            last_keyword = "";
            if rest.trim().is_empty() {
                errors.push(format!("{}: Scenario has no name", where_(n)));
            }
            continue;
        }

        // Step lines.
        let kw = ["Given", "When", "Then", "And", "But"]
            .into_iter()
            .find(|k| t.starts_with(*k));
        if let Some(kw) = kw {
            if !in_scenario {
                errors.push(format!("{}: step outside a Scenario", where_(n)));
            }
            steps_in_scenario = steps_in_scenario.saturating_add(1);
            lint_step(
                kw,
                t[kw.len()..].trim(),
                &where_(n),
                &mut last_keyword,
                errors,
            );
            continue;
        }

        // Allow doc-strings / data-table / tags / Background / Examples.
        if t.starts_with('@')
            || t.starts_with('|')
            || t.starts_with('"')
            || t.starts_with("Background:")
            || t.starts_with("Examples:")
            || t.starts_with("Rule:")
        {
            continue;
        }

        errors.push(format!("{}: unrecognized Gherkin line: {t:?}", where_(n)));
    }

    if !saw_feature {
        errors.push(format!("{}: no Feature: declaration", path.display()));
    }
    if in_scenario && steps_in_scenario == 0 {
        errors.push(format!("{}: last scenario has no steps", path.display()));
    }
    scenarios
}

/// Lint one Given/When/Then/And/But step line. `body` is the step text after
/// the keyword and `loc` its `file:line`. `last_keyword` is threaded so an
/// `And`/`But` inherits the keyword it continues (for the EARS Then check).
fn lint_step(
    kw: &'static str,
    body: &str,
    loc: &str,
    last_keyword: &mut &'static str,
    errors: &mut Vec<String>,
) {
    if body.is_empty() {
        errors.push(format!("{loc}: empty {kw} step"));
    }
    // Resolve And/But to the keyword they continue.
    let effective = if kw == "And" || kw == "But" {
        *last_keyword
    } else {
        kw
    };
    if kw != "And" && kw != "But" {
        *last_keyword = kw;
    }
    // EARS: a Then (or And after Then) asserts a system response. We require it
    // to name the system ("the service"/"the todo"/"the list"/"the response") —
    // i.e. an observable response, not an action. Keeps Then clauses
    // outcome-shaped.
    if effective == "Then" {
        let mentions_system = ["the service", "the todo", "the list", "the response"]
            .iter()
            .any(|s| body.contains(s));
        if !mentions_system {
            errors.push(format!(
                "{loc}: Then step is not EARS response-shaped (name the system: \
                 'the service/todo/list/response shall …'): {body:?}"
            ));
        }
    }
}

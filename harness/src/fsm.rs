//! FSM regen — generate the lifecycle `.{tla,cfg}` from the FSM source.
//!
//! Opt-in (R2/C2): `regen`/`check-spec-sync` only run when `[fsm]` is configured
//! (and the load-time shape check already failed if a conventional FSM source
//! exists without `[fsm]`). State/event enum names and all paths come from
//! config, so the parser is not wedded to `TodoState`/`Event`.
use anyhow::{anyhow, Context, Result};

use crate::config::{Config, Fsm as FsmCfg};
use crate::fsm_render::{render_cfg, render_tla};

/// One `(source, event) -> target` transition extracted from the FSM function.
#[derive(Debug, Clone)]
pub(crate) struct Transition {
    pub(crate) source: String,
    pub(crate) event: String,
    pub(crate) target: String,
}

/// The extracted FSM: (states, events, transitions), each in source order.
pub(crate) type Fsm = (Vec<String>, Vec<String>, Vec<Transition>);

/// Parse a Rust file and extract the FSM: the state/event variant lists
/// (enum-declaration order) and the `Some`-returning arms of the named
/// transition function. `read_fsm_source` slices out just those three items, so
/// unrelated code in the file never has to parse as plain Rust.
pub(crate) fn parse_fsm_from(
    path: &str,
    fn_name: &str,
    state_enum: &str,
    event_enum: &str,
) -> Result<Fsm> {
    let src = read_fsm_source(path, fn_name, state_enum, event_enum)?;
    let file =
        syn::parse_file(&src).with_context(|| format!("parse extracted FSM items of {path}"))?;

    let mut states: Vec<String> = Vec::new();
    let mut events: Vec<String> = Vec::new();
    let mut transitions: Vec<Transition> = Vec::new();

    for item in &file.items {
        match item {
            syn::Item::Enum(e) if e.ident == state_enum => {
                states = e.variants.iter().map(|v| v.ident.to_string()).collect();
            }
            syn::Item::Enum(e) if e.ident == event_enum => {
                events = e.variants.iter().map(|v| v.ident.to_string()).collect();
            }
            syn::Item::Fn(f) if f.sig.ident == fn_name => {
                transitions = extract_transitions(&f.block)?;
            }
            _ => {}
        }
    }

    if states.is_empty() || events.is_empty() {
        return Err(anyhow!(
            "{path}: could not find {state_enum}/{event_enum} enums"
        ));
    }
    if transitions.is_empty() {
        return Err(anyhow!(
            "{path}: could not extract transitions from {fn_name}()"
        ));
    }
    Ok((states, events, transitions))
}

/// Slice a balanced `{ … }` block starting at the first `{` at/after `from`.
/// Returns the byte index just past the closing brace.
fn block_end(src: &str, from: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let open = src[from..].find('{')? + from;
    let mut depth = 0_i32;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract just the FSM-relevant items from the source so syn can parse them as
/// plain Rust: the state + event enums and the named transition function.
/// Anything else in the file is never handed to syn — only these three items
/// are sliced out by name.
fn read_fsm_source(
    path: &str,
    fn_name: &str,
    state_enum: &str,
    event_enum: &str,
) -> Result<String> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let mut out = String::new();

    // The two enums (match the `enum <Name>` keyword + following block).
    for name in [state_enum, event_enum] {
        let needle = format!("enum {name}");
        if let Some(pos) = raw.find(&needle) {
            if let Some(end) = block_end(&raw, pos) {
                out.push_str("pub ");
                out.push_str(&raw[pos..end]);
                out.push_str("\n\n");
            }
        }
    }

    // The transition function. Find `fn <fn_name>` as a WHOLE word — the name
    // must be followed by `(` or whitespace, so searching for `next` does not
    // accidentally bind to `next_spec` (a prefix match). Then take from its
    // signature start through its body block.
    let fn_needle = format!("fn {fn_name}");
    let fn_pos = {
        let bytes = raw.as_bytes();
        let mut from = 0_usize;
        loop {
            let Some(rel) = raw[from..].find(&fn_needle) else {
                return Err(anyhow!("{path}: function {fn_name} not found"));
            };
            let at = from + rel;
            let after = at + fn_needle.len();
            // The char right after the name must not continue the identifier.
            let boundary = bytes
                .get(after)
                .is_none_or(|&b| !(b.is_ascii_alphanumeric() || b == b'_'));
            if boundary {
                break at;
            }
            from = after;
        }
    };
    let fn_end =
        block_end(&raw, fn_pos).ok_or_else(|| anyhow!("{path}: unbalanced braces in {fn_name}"))?;
    // Find the `(` … `)` of the param list, then the body `{`.
    let sig = &raw[fn_pos..fn_end];
    let params_end = sig
        .find(')')
        .ok_or_else(|| anyhow!("{path}: no params in {fn_name}"))?;
    let body_open = sig[params_end..]
        .find('{')
        .map(|o| params_end + o)
        .ok_or_else(|| anyhow!("{path}: no body in {fn_name}"))?;
    out.push_str("pub fn ");
    out.push_str(fn_name);
    out.push_str(&sig[fn_needle.len()..=params_end]); // params incl. trailing ')'
    out.push(' ');
    out.push_str(&sig[body_open..]); // body block

    Ok(out)
}

/// Parse the configured FSM source — the canonical FSM used by `regen`.
fn parse_fsm(fsm: &FsmCfg, root: &std::path::Path) -> Result<Fsm> {
    parse_fsm_from(
        &root.join(&fsm.source).to_string_lossy(),
        &fsm.fn_name,
        &fsm.state_enum,
        &fsm.event_enum,
    )
}

/// Last path segment ident of an expression like `TodoState::Done` → "Done".
fn expr_last_ident(e: &syn::Expr) -> Option<String> {
    match e {
        syn::Expr::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// Last path segment ident of a pattern like `TodoState::Active` → "Active".
fn pat_last_ident(p: &syn::Pat) -> Option<String> {
    match p {
        syn::Pat::Path(pp) => pp.path.segments.last().map(|s| s.ident.to_string()),
        syn::Pat::TupleStruct(ts) => ts.path.segments.last().map(|s| s.ident.to_string()),
        syn::Pat::Ident(i) => Some(i.ident.to_string()),
        _ => None,
    }
}

/// Pull the `Some`-returning arms out of `match (state, ev) { … }`.
fn extract_transitions(block: &syn::Block) -> Result<Vec<Transition>> {
    let match_expr = block.stmts.iter().find_map(|s| match s {
        syn::Stmt::Expr(syn::Expr::Match(m), _) => Some(m),
        _ => None,
    });
    let m = match_expr.ok_or_else(|| anyhow!("next(): body is not a single match expression"))?;

    let mut out = Vec::new();
    for arm in &m.arms {
        // Pattern must be a 2-tuple `(state, event)`.
        let syn::Pat::Tuple(tuple) = &arm.pat else {
            continue;
        };
        if tuple.elems.len() != 2 {
            continue;
        }
        // Body must be `Some(TodoState::Target)`; skip `None` / wildcard arms.
        let syn::Expr::Call(call) = &*arm.body else {
            continue;
        };
        if expr_last_ident(&call.func).as_deref() != Some("Some") {
            continue;
        }
        let Some(target_expr) = call.args.first() else {
            continue;
        };
        let Some(target) = expr_last_ident(target_expr) else {
            continue;
        };

        // A `Some` arm uses single-variant patterns for state + event.
        let state_pat = &tuple.elems[0];
        let event_pat = &tuple.elems[1];
        if matches!(state_pat, syn::Pat::Wild(_) | syn::Pat::Or(_))
            || matches!(event_pat, syn::Pat::Wild(_) | syn::Pat::Or(_))
        {
            continue;
        }
        let (Some(source), Some(event)) = (pat_last_ident(state_pat), pat_last_ident(event_pat))
        else {
            continue;
        };
        out.push(Transition {
            source,
            event,
            target,
        });
    }
    Ok(out)
}

pub(crate) fn regen(cfg: &Config, check: bool) -> Result<()> {
    // Opt-in (R2/C2): no [fsm] ⇒ no lifecycle spec to regenerate. The load-time
    // shape check already fails if a conventional FSM source exists unconfigured,
    // so this skip is safe.
    let Some(fsm) = cfg.raw.fsm.as_ref() else {
        println!("regen: [fsm] not configured (skip — no FSM in this project)");
        return Ok(());
    };
    let tla = cfg.path(&format!(
        "{}/{}.tla",
        cfg.raw.docs.spec_dir, fsm.generated_stem
    ));
    let tla_cfg = cfg.path(&format!(
        "{}/{}.cfg",
        cfg.raw.docs.spec_dir, fsm.generated_stem
    ));

    let (states, events, transitions) = parse_fsm(fsm, &cfg.root)?;
    let generated = render_tla(
        &fsm.generated_stem,
        &format!("{}::{}", fsm.source, fsm.fn_name),
        &states,
        &events,
        &transitions,
    );
    let generated_cfg = render_cfg(&states, &events);

    if check {
        let tla_disk = std::fs::read_to_string(&tla).unwrap_or_default();
        let cfg_disk = std::fs::read_to_string(&tla_cfg).unwrap_or_default();
        if tla_disk != generated {
            return Err(anyhow!(
                "regen drift: {} does not match the FSM source. Run `dhx regen` and commit.",
                tla.display()
            ));
        }
        if cfg_disk != generated_cfg {
            return Err(anyhow!(
                "regen drift: {} does not match the FSM source (CONSTANTS stale). \
                 Run `dhx regen` and commit.",
                tla_cfg.display()
            ));
        }
        println!(
            "✓ regen --check: {} + {} up-to-date ({} transitions)",
            tla.display(),
            tla_cfg.display(),
            transitions.len()
        );
        return Ok(());
    }

    if let Some(parent) = tla.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&tla, &generated).with_context(|| format!("write {}", tla.display()))?;
    std::fs::write(&tla_cfg, &generated_cfg)
        .with_context(|| format!("write {}", tla_cfg.display()))?;
    println!(
        "✓ regen: wrote {} + {} ({} states, {} events, {} transitions)",
        tla.display(),
        tla_cfg.display(),
        states.len(),
        events.len(),
        transitions.len()
    );
    Ok(())
}

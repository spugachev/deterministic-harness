//! Fuzz target for the untrusted transfer-command parser (REQ-001).
//!
//! The parser is the trust boundary: it takes ARBITRARY bytes and must always
//! return a typed `Ok`/`Err`, never panic. libFuzzer feeds it adversarial input;
//! a panic (or hang) is a finding. We also re-parse any successfully parsed
//! command's canonical rendering and assert it round-trips, so the fuzzer pins
//! correctness, not just absence of panics.
#![no_main]

use libfuzzer_sys::fuzz_target;

use core::domain::parse::{parse_transfer, parse_transfer_str};

fuzz_target!(|data: &[u8]| {
    // Primary property: never panics on arbitrary bytes.
    if let Ok(cmd) = parse_transfer(data) {
        // Round-trip: a parsed command re-renders to a line that parses back to
        // the same command.
        let line = format!(
            "TRANSFER {} {} {} {}",
            cmd.from, cmd.to, cmd.amount_cents, cmd.key
        );
        let reparsed = parse_transfer_str(&line).expect("rendered command must re-parse");
        assert_eq!(reparsed, cmd);
    }
});

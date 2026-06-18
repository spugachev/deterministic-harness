---
id: REQ-001
title: Parse a transfer command from untrusted single-line text
status: active
acceptance:
  - When given a well-formed line "TRANSFER <from> <to> <amount> <key>" the service shall return a typed command with the parsed fields
  - When given malformed, empty, overlong, non-numeric, overflowing, or wrong-arity input the service shall return a typed error and shall never panic (verified=proptest)
implements_in:
  gherkin: [spec/features/parse.feature]
  code: [crates/core/src/domain/parse.rs::parse_transfer]
  proptest: [crates/core/src/domain/parse.rs]
  dst: [crates/api/tests/dst.rs]
---

## Rationale

The transfer protocol is a single line of untrusted text. The parser is the
trust boundary: it accepts arbitrary bytes and must always terminate with a
typed `Ok`/`Err`, never a panic, regardless of how hostile the input is. This
totality is asserted by a proptest law over arbitrary byte vectors and exercised
far more widely by the `parse_input` fuzz target.

---
id: REQ-001
title: RESP command parser is total and panic-free
status: active
acceptance:
  - When given a well-formed GET, SET, or DEL request the system shall return the corresponding typed command
  - When given malformed, truncated, wrong-arity, non-UTF-8, or overlong input the system shall return an error and shall never panic (verified=proptest)
implements_in:
  gherkin: [spec/features/resp_parser.feature]
  code: [crates/core/src/domain/resp.rs::parse]
  proptest: [crates/core/src/domain/resp.rs]
  fuzz: [fuzz/fuzz_targets/parse_resp.rs]
---

## Rationale

The parser is the only surface that consumes untrusted bytes, so it must be a
pure, total function: every input maps to either a typed [`Command`] or a
[`ParseError`], with no panic, overflow, or unbounded allocation (a colossal
length prefix becomes `TooLong`, not an OOM). Panic-freedom is asserted by a
proptest over arbitrary byte vectors and exhaustively explored by the
`parse_resp` fuzz target.

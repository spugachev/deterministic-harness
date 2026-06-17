---
id: REQ-004
title: Booking parsing is total
status: active
acceptance:
  - The booking parser shall be total and panic-free for any input string (verified=proptest)
  - A well-formed "<section>:<qty>" string shall round-trip through the parser unchanged (verified=proptest)
implements_in:
  code: [crates/core/src/domain/parse.rs::parse_booking]
---

## Rationale

`parse::parse_booking(&str)` turns untrusted text into a typed `Booking` or a
typed `ParseError`, and is **total and panic-free for ANY input** — empty,
unicode, embedded NULs, oversized numbers. No `unwrap`/indexing/slicing (clippy's
restriction lints forbid them in this crate). A proptest law over `".*"` proves
totality; a second law proves that any rendered well-formed booking parses back
to itself.

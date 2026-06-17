---
id: REQ-002
title: Capacity never oversells
status: active
acceptance:
  - When a reservation is granted the new held count shall never exceed capacity (verified=kani)
  - The reserve function shall be total and panic-free for every (capacity, held, qty) (verified=proptest)
implements_in:
  code: [crates/core/src/domain/capacity.rs::reserve]
  kani: [crates/core/src/domain/capacity.rs::reserve_never_oversells]
---

## Rationale

This is the ONE safety property of the service: **never oversell**.
`capacity::reserve(capacity, held, qty)` returns the new held count only when
`held + qty <= capacity`, refusing otherwise with `OverError::Insufficient`. All
arithmetic is checked/saturating, so the function is total — no input panics or
overflows (including `held > capacity` corruption and `u32` overflow).

The invariant `held' <= capacity` is proven two ways: a proptest law over random
inputs, and a `#[kani::proof]` (`reserve_never_oversells`) that model-checks it
over the entire symbolic `u32 × u32 × u32` space — the strongest available
statement of "never oversell".

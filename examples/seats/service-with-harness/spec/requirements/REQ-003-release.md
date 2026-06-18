---
id: REQ-003
title: Release frees a hold and is idempotent
status: active
acceptance:
  - When a client releases a live unconfirmed hold the service shall return its seats to available
  - When a client releases an expired or unknown hold the service shall treat it as a no-op (verified=proptest)
implements_in:
  gherkin: [spec/features/holds.feature]
  code:
    - crates/core/src/domain/seats.rs::release
    - crates/core/src/domain/hold.rs::next
  tla: [spec/tla/Lifecycle.tla]
---

## Rationale

Releasing a live hold moves it `Held → Released` and frees its seats. Because an
expired or unknown hold already holds no live seats, releasing it is a no-op —
the operation is idempotent, which makes client retries safe (HTTP `204 No
Content` regardless). `SeatMap::release` returns `false` when nothing was
removed.

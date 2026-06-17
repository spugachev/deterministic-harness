---
id: REQ-001
title: Grant clamps a request to the remaining budget
status: active
acceptance:
  - When the requested amount is at most the remaining budget the system shall grant the full request
  - When the requested amount exceeds the remaining budget the system shall grant only the remaining amount
implements_in:
  gherkin: [spec/features/example.feature]
  code: [crates/core/src/domain/example.rs::grant]
---

## Rationale

A throwaway starter requirement so the freshly-scaffolded project is green and
demonstrates the spec → code → test flow: an EARS requirement, its executable
BDD scenarios (`spec/features/example.feature`), the pure implementation
(`domain::example::grant`), and a property test for the law. **Replace this with
your own first requirement.**

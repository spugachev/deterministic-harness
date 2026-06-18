Feature: Grant clamps a request to the remaining budget (REQ-001)
  A throwaway starter feature so the scaffold is green and shows the EARS →
  Gherkin → step shape. Replace it with your own. The steps drive the pure
  `domain::example::grant` directly — no IO, no HTTP needed to observe it.

  Scenario: REQ-001 — a request within budget is granted in full
    Given a remaining budget of 10
    When a request for 3 is made
    Then the system shall grant 3

  Scenario: REQ-001 — a request over budget is clamped to what remains
    Given a remaining budget of 3
    When a request for 10 is made
    Then the system shall grant 3

  Scenario: REQ-001 — a request exactly at the budget is granted in full
    Given a remaining budget of 5
    When a request for 5 is made
    Then the system shall grant 5

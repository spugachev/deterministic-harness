Feature: Query balance and lifecycle state (REQ-007)
  Callers can read an account's balance and lifecycle state; an unknown account
  is reported as unknown rather than a fabricated zero balance.

  Scenario: REQ-007 — querying an existing account reports balance and state
    Given an account 1 with balance 100
    When account 1 is queried
    Then the query shall report balance 100 and state "Open"

  Scenario: REQ-007 — querying an unknown account reports unknown
    Given an account 1 with balance 100
    When account 9 is queried
    Then the query shall report the account is unknown

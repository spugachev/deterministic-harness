Feature: Concurrent transfers stay safe (REQ-006)
  Many transfers racing on the same accounts must never double-spend, never
  drive a balance negative, and never violate conservation. The scenario models
  the serialized effect the locked ledger guarantees.

  Scenario: REQ-006 — racing withdrawals never overdraw the source
    Given an account 1 with balance 100
    And an account 2 with balance 0
    When a transfer of 60 from 1 to 2 with key "r1" is attempted
    And a transfer of 60 from 1 to 2 with key "r2" is attempted
    Then exactly one transfer shall be applied and the other rejected
    And account 1 shall have balance 40
    And the total balance shall be 100

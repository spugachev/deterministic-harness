Feature: Account lifecycle Open to Frozen to Closed (REQ-005)
  An account moves Open -> Frozen -> Closed. Frozen and Closed reject all
  transfers; Closed is terminal. Transitions are explicit operations.

  Scenario: REQ-005 — an open account can be frozen
    Given an account 1 with balance 100
    When account 1 is frozen
    Then account 1 shall be in state "Frozen"

  Scenario: REQ-005 — a frozen account rejects transfers out
    Given an account 1 with balance 100
    And an account 2 with balance 0
    When account 1 is frozen
    And a transfer of 10 from 1 to 2 with key "f1" is attempted
    Then the transfer shall be rejected

  Scenario: REQ-005 — a closed account is terminal and cannot be reopened
    Given an account 1 with balance 100
    When account 1 is closed
    And account 1 is unfrozen
    Then the lifecycle transition shall be rejected
    And account 1 shall be in state "Closed"

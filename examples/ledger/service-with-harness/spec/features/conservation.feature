Feature: The sum of all balances is invariant (REQ-003)
  Money is conserved: a successful transfer moves money, a rejected one changes
  nothing, and the total across all accounts never changes either way.

  Scenario: REQ-003 — an applied transfer conserves the total
    Given an account 1 with balance 100
    And an account 2 with balance 50
    When a transfer of 40 from 1 to 2 with key "c1" is attempted
    Then the transfer shall be applied
    And the total balance shall be 150

  Scenario: REQ-003 — a rejected transfer conserves the total
    Given an account 1 with balance 100
    And an account 2 with balance 50
    When a transfer of 9999 from 1 to 2 with key "c2" is attempted
    Then the transfer shall be rejected
    And the total balance shall be 150

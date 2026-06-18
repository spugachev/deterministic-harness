Feature: Transfer moves money only when valid (REQ-002)
  A transfer moves cents between two open accounts when the source has funds,
  and is otherwise a typed no-op. Balances never go negative.

  Scenario: REQ-002 — a valid transfer moves the amount
    Given an account 1 with balance 100
    And an account 2 with balance 0
    When a transfer of 30 from 1 to 2 with key "k1" is attempted
    Then the transfer shall be applied
    And account 1 shall have balance 70
    And account 2 shall have balance 30

  Scenario: REQ-002 — insufficient funds is rejected with no state change
    Given an account 1 with balance 100
    And an account 2 with balance 0
    When a transfer of 9999 from 1 to 2 with key "k2" is attempted
    Then the transfer shall be rejected
    And account 1 shall have balance 100

  Scenario: REQ-002 — a zero amount is rejected
    Given an account 1 with balance 100
    And an account 2 with balance 0
    When a transfer of 0 from 1 to 2 with key "k3" is attempted
    Then the transfer shall be rejected

  Scenario: REQ-002 — a self-transfer is rejected
    Given an account 1 with balance 100
    When a transfer of 10 from 1 to 1 with key "k4" is attempted
    Then the transfer shall be rejected

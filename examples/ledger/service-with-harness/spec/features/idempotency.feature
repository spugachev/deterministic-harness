Feature: Transfers are idempotent by key (REQ-004)
  Re-submitting a transfer with an already-applied key is a no-op that returns
  the original outcome — money moves at most once per key.

  Scenario: REQ-004 — replaying a key moves money only once
    Given an account 1 with balance 100
    And an account 2 with balance 0
    When a transfer of 30 from 1 to 2 with key "dup" is attempted
    And a transfer of 30 from 1 to 2 with key "dup" is attempted
    Then the transfer shall be a duplicate
    And account 1 shall have balance 70
    And account 2 shall have balance 30

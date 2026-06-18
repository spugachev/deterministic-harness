Feature: Key-value state machine (REQ-002)
  Committed commands apply deterministically to an in-memory map: SET writes,
  GET reads, DEL removes, and the result is a pure function of (state, command).

  Scenario: REQ-002 — SET then GET returns the value
    Given an empty key-value store
    When the committed command SET "a" "1" is applied
    And the committed command GET "a" is applied
    Then the system shall report the value "1"

  Scenario: REQ-002 — GET of an absent key reports no value
    Given an empty key-value store
    When the committed command GET "missing" is applied
    Then the system shall report no value

  Scenario: REQ-002 — DEL removes a key
    Given a store holding "a" = "1"
    When the committed command DEL "a" is applied
    And the committed command GET "a" is applied
    Then the system shall report no value

  Scenario: REQ-002 — SET overwrites an existing value
    Given a store holding "a" = "1"
    When the committed command SET "a" "2" is applied
    And the committed command GET "a" is applied
    Then the system shall report the value "2"

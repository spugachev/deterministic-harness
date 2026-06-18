Feature: Confirming a hold books its seats (REQ-003)
  Confirming a live hold commits its seats permanently; confirming an expired,
  unknown, or already-confirmed hold fails.

  Scenario: REQ-003 a live hold is confirmed
    Given a venue with 5 seats
    And a hold 1 for 2 seats
    When a client confirms hold 1
    Then the confirmation shall succeed
    And the service shall report 3 seats available

  Scenario: REQ-003 confirming an unknown hold fails
    Given a venue with 5 seats
    When a client confirms hold 99
    Then the confirmation shall fail

  Scenario: REQ-003 a hold cannot be confirmed twice
    Given a venue with 5 seats
    And a hold 1 for 2 seats
    When a client confirms hold 1
    Then the confirmation shall succeed
    When a client confirms hold 1
    Then the confirmation shall fail

  Scenario: REQ-003 an expired hold cannot be confirmed
    Given a venue with 5 seats
    And a hold 1 for 2 seats
    And the time advances past the hold TTL
    When a client confirms hold 1
    Then the confirmation shall fail

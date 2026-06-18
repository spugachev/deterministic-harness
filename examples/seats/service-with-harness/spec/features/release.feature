Feature: Releasing a hold returns its seats (REQ-004)
  Releasing an unconfirmed hold frees its seats immediately; releasing an
  unknown or expired hold is an idempotent no-op.

  Scenario: REQ-004 releasing a live hold frees its seats
    Given a venue with 5 seats
    And a hold 1 for 3 seats
    When a client releases hold 1
    Then the service shall report 5 seats available

  Scenario: REQ-004 releasing an unknown hold is a no-op
    Given a venue with 5 seats
    And a hold 1 for 3 seats
    When a client releases hold 99
    Then the service shall report 2 seats available

  Scenario: REQ-004 releasing the same hold twice is a no-op
    Given a venue with 5 seats
    And a hold 1 for 3 seats
    When a client releases hold 1
    And a client releases hold 1
    Then the service shall report 5 seats available

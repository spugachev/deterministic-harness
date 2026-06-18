Feature: Unconfirmed holds expire (REQ-005)
  A hold reserves its seats only within its TTL; once the TTL elapses the seats
  are freed lazily against the current clock.

  Scenario: REQ-005 a hold within its TTL keeps its seats reserved
    Given a venue with 4 seats
    And a hold 1 for 4 seats
    Then the service shall report 0 seats available

  Scenario: REQ-005 a hold past its TTL frees its seats
    Given a venue with 4 seats
    And a hold 1 for 4 seats
    And the time advances past the hold TTL
    Then the service shall report 4 seats available

Feature: Seat holds never oversell (REQ-001)
  A client may hold seats only when enough are free; otherwise the hold is
  rejected. The steps drive the pure SeatMap directly — no HTTP needed.

  Scenario: REQ-001 a hold within capacity is granted
    Given a venue with 10 seats
    When a client requests a hold 1 for 4 seats
    Then the service shall grant the hold
    And the service shall report 6 seats available

  Scenario: REQ-001 a hold beyond remaining capacity is rejected
    Given a venue with 3 seats
    And a hold 1 for 2 seats
    When a client requests a hold 2 for 2 seats
    Then the service shall reject the hold

  Scenario: REQ-001 the exact last seats can still be held
    Given a venue with 3 seats
    And a hold 1 for 2 seats
    When a client requests a hold 2 for 1 seats
    Then the service shall grant the hold
    And the service shall report 0 seats available

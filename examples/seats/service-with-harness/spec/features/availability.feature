Feature: Availability query (REQ-006)
  The service reports how many seats are free: capacity minus confirmed minus
  currently-held. The steps drive the pure SeatMap ledger directly.

  Scenario: REQ-006 availability reflects holds and confirmations
    Given an event with 10 seats
    When a client holds 2 seats with id 1 at time 0 expiring at 100
    And the client confirms hold 1 at time 1
    And a client holds 3 seats with id 2 at time 1 expiring at 100
    Then the available count at time 1 shall be 5

  Scenario: REQ-006 a fresh event reports full availability
    Given an event with 10 seats
    Then the available count at time 0 shall be 10

Feature: The capacity invariant is never violated (REQ-005)
  Confirmed plus currently-held seats must never exceed capacity, under any
  sequence of operations. The steps drive the pure SeatMap ledger directly.

  Scenario: REQ-005 holds plus confirmations never exceed capacity
    Given an event with 5 seats
    When a client holds 3 seats with id 1 at time 0 expiring at 100
    And the client confirms hold 1 at time 1
    And a client holds 2 seats with id 2 at time 1 expiring at 100
    And a client holds 1 seats with id 3 at time 1 expiring at 100
    Then the last hold shall be rejected for insufficient availability
    And confirmed plus held at time 1 shall not exceed 5

  Scenario: REQ-005 the last seats are not double-granted
    Given an event with 1 seats
    When a client holds 1 seats with id 1 at time 0 expiring at 100
    And a client holds 1 seats with id 2 at time 0 expiring at 100
    Then the last hold shall be rejected for insufficient availability
    And confirmed plus held at time 0 shall not exceed 1

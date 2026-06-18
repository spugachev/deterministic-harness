Feature: Seat hold lifecycle (REQ-001, REQ-002, REQ-003, REQ-004)
  A client holds seats, then confirms, releases, or lets the hold expire. The
  steps drive the pure SeatMap ledger and the hold FSM directly — no HTTP needed.

  Scenario: REQ-001 a request within availability is granted
    Given an event with 10 seats
    When a client holds 3 seats with id 1 at time 0 expiring at 100
    Then the hold shall be granted
    And the available count at time 0 shall be 7

  Scenario: REQ-001 a request beyond availability is rejected
    Given an event with 5 seats
    When a client holds 4 seats with id 1 at time 0 expiring at 100
    And a client holds 2 seats with id 2 at time 0 expiring at 100
    Then the last hold shall be rejected for insufficient availability

  Scenario: REQ-001 a zero-seat request is rejected
    Given an event with 5 seats
    When a client holds 0 seats with id 1 at time 0 expiring at 100
    Then the last hold shall be rejected for zero seats

  Scenario: REQ-002 confirming a live hold books its seats
    Given an event with 10 seats
    When a client holds 4 seats with id 1 at time 0 expiring at 100
    And the client confirms hold 1 at time 50
    Then the confirmation shall succeed
    And the confirmed count shall be 4

  Scenario: REQ-002 confirming an expired hold is rejected
    Given an event with 10 seats
    When a client holds 4 seats with id 1 at time 0 expiring at 100
    And the client confirms hold 1 at time 100
    Then the confirmation shall be rejected as unknown

  Scenario: REQ-003 releasing a live hold frees its seats
    Given an event with 10 seats
    When a client holds 4 seats with id 1 at time 0 expiring at 100
    And the client releases hold 1 at time 10
    Then the release shall report a hold was freed
    And the available count at time 10 shall be 10

  Scenario: REQ-003 releasing an unknown hold is a no-op
    Given an event with 10 seats
    When the client releases hold 99 at time 0
    Then the release shall report no hold was freed

  Scenario: REQ-004 a hold's seats are freed once its TTL elapses
    Given an event with 10 seats
    When a client holds 6 seats with id 1 at time 0 expiring at 100
    Then the available count at time 50 shall be 4
    And the available count at time 100 shall be 10

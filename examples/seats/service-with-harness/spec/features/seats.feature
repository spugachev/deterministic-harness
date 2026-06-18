Feature: Seat reservation for a single event
  The seat-reservation domain: clients hold seats, confirm or release them, and
  holds expire after a TTL — all without ever overbooking the event's capacity.
  The steps drive the pure `domain::reservation::Reservation` directly with a
  fixed Clock and a seeded IdGen — no IO, no HTTP needed to observe it.

  Scenario: REQ-001 — a hold within availability is granted with an id
    Given an event with capacity 10 and a hold TTL of 60 seconds
    And the current time is 1000
    When a client holds 3 seats
    Then the system shall grant a hold for 3 seats
    And the system shall report 7 seats available

  Scenario: REQ-001 — a hold beyond availability is rejected
    Given an event with capacity 10 and a hold TTL of 60 seconds
    And the current time is 1000
    And a client holds 8 seats
    When a client holds 5 seats
    Then the system shall reject the hold for insufficient availability

  Scenario: REQ-001 — a hold for zero seats is rejected
    Given an event with capacity 10 and a hold TTL of 60 seconds
    And the current time is 1000
    When a client holds 0 seats
    Then the system shall reject the hold for zero seats

  Scenario: REQ-002 — confirming a live hold books its seats permanently
    Given an event with capacity 10 and a hold TTL of 60 seconds
    And the current time is 1000
    And a client holds 4 seats
    When the client confirms the hold at time 1010
    Then the system shall report the confirmation succeeded
    And the system shall report 4 seats confirmed

  Scenario: REQ-002 — confirming an expired hold is rejected
    Given an event with capacity 10 and a hold TTL of 60 seconds
    And the current time is 1000
    And a client holds 4 seats
    When the client confirms the hold at time 1100
    Then the system shall reject the confirmation

  Scenario: REQ-003 — releasing a live hold returns its seats
    Given an event with capacity 10 and a hold TTL of 60 seconds
    And the current time is 1000
    And a client holds 4 seats
    When the client releases the hold at time 1005
    Then the system shall report 10 seats available

  Scenario: REQ-003 — releasing an unknown hold is a no-op
    Given an event with capacity 10 and a hold TTL of 60 seconds
    And the current time is 1000
    When the client releases hold id 999 at time 1005
    Then the system shall report the release was a no-op

  Scenario: REQ-004 — an unconfirmed hold expires after its TTL
    Given an event with capacity 10 and a hold TTL of 60 seconds
    And the current time is 1000
    And a client holds 10 seats
    When the time advances to 1061
    Then the system shall report 10 seats available

  Scenario: REQ-005 — two clients racing for the last seats do not overbook
    Given an event with capacity 10 and a hold TTL of 60 seconds
    And the current time is 1000
    And a client holds 8 seats
    When a client holds 2 seats
    And a client holds 1 seats
    Then the system shall grant the hold for 2 seats
    And the system shall reject the hold for insufficient availability
    And the system shall keep confirmed plus held seats at most 10

  Scenario: REQ-006 — availability reflects confirmed and held seats
    Given an event with capacity 10 and a hold TTL of 60 seconds
    And the current time is 1000
    And a client holds 2 seats
    And the client confirms the hold at time 1001
    When a client holds 3 seats
    Then the system shall report 5 seats available

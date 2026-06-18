Feature: Concurrent holds never both win the last seats (REQ-007)
  Clients racing for the last seats must not both succeed; only as many win as
  seats remain. Operations are serialized (ADR-0002), so a race reduces to a
  serial sequence; these steps drive that serialized sequence against the core.

  Scenario: REQ-007 two clients racing for the last seat — only one wins
    Given a venue with 1 seats
    When a client requests a hold 1 for 1 seats
    Then the service shall grant the hold
    When a client requests a hold 2 for 1 seats
    Then the service shall reject the hold

  Scenario: REQ-007 a race for two seats grants exactly what remains
    Given a venue with 2 seats
    When a client requests a hold 1 for 2 seats
    Then the service shall grant the hold
    When a client requests a hold 2 for 1 seats
    Then the service shall reject the hold

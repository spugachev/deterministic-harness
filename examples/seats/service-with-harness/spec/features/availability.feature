Feature: Availability query (REQ-006)
  The service reports how many seats remain: capacity minus confirmed and
  currently-held seats.

  Scenario: REQ-006 a fresh venue reports full availability
    Given a venue with 8 seats
    Then the service shall report 8 seats available

  Scenario: REQ-006 a held and a confirmed seat both reduce availability
    Given a venue with 8 seats
    And a hold 1 for 2 seats
    And a hold 2 for 3 seats
    When a client confirms hold 1
    Then the service shall report 3 seats available

Feature: Hold lifecycle is one-way (REQ-002)
  A held hold makes exactly one terminal transition; terminal states reject
  every further event. These steps drive the pure FSM `next` directly.

  Scenario: REQ-002 a held hold can be confirmed
    Given a hold in the Held state
    When the confirm event fires
    Then the hold shall move to the Confirmed state

  Scenario: REQ-002 a held hold can be released
    Given a hold in the Held state
    When the release event fires
    Then the hold shall move to the Released state

  Scenario: REQ-002 a held hold can expire
    Given a hold in the Held state
    When the expire event fires
    Then the hold shall move to the Expired state

  Scenario: REQ-002 a confirmed hold rejects further events
    Given a hold in the Confirmed state
    When the release event fires
    Then the transition shall be rejected

  Scenario: REQ-002 a released hold rejects further events
    Given a hold in the Released state
    When the confirm event fires
    Then the transition shall be rejected

  Scenario: REQ-002 an expired hold rejects further events
    Given a hold in the Expired state
    When the confirm event fires
    Then the transition shall be rejected

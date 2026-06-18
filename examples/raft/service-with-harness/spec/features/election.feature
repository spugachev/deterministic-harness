Feature: Leader election (REQ-003)
  A follower that times out becomes a candidate, increments its term, and
  requests votes; a candidate with a majority becomes the single leader.

  Scenario: REQ-003 — a cluster elects exactly one leader
    Given a fresh cluster of 3 nodes
    When the cluster runs until a leader is elected
    Then the system shall have exactly one leader

  Scenario: REQ-003 — election produces at most one leader per term
    Given a fresh cluster of 5 nodes
    When the cluster runs until a leader is elected
    Then the system shall have at most one leader per term

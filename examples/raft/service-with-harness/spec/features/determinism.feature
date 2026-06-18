Feature: Deterministic, seed-reproducible runs (REQ-006)
  All time and randomness flow through the Clock/Rng/IdGen ports, so a whole
  cluster run is reproducible: the same seed always yields the same outcome.

  Scenario: REQ-006 — the same seed elects the same leader
    Given a fresh cluster of 3 nodes
    When the cluster runs until a leader is elected
    Then the system shall elect the same leader for the same seed

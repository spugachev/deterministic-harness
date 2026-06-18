Feature: Raft safety invariants (REQ-005)
  The safety properties must hold under any interleaving: at most one leader per
  term (Election Safety), logs that agree wherever index and term agree (Log
  Matching), and a non-leader can never commit (no split-brain).

  Scenario: REQ-005 — Election Safety holds after an election
    Given a fresh cluster of 5 nodes
    When the cluster runs until a leader is elected
    Then the system shall have at most one leader per term

  Scenario: REQ-005 — Log Matching holds after replication
    Given a fresh cluster of 3 nodes
    When the cluster runs until a leader is elected
    And a client proposes SET "k" "v" to the leader
    Then the system shall keep all node logs matching

  Scenario: REQ-005 — a non-leader cannot commit a write (no split-brain)
    Given a fresh cluster of 3 nodes
    When the cluster runs until a leader is elected
    And a non-leader node receives a client proposal
    Then the system shall reject the proposal

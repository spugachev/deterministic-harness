Feature: Log replication and commit (REQ-004)
  The leader appends client commands and replicates them; an entry committed by
  a majority is applied to the key-value state machine on every node, in order.

  Scenario: REQ-004 — a committed write reaches every node
    Given a fresh cluster of 3 nodes
    When the cluster runs until a leader is elected
    And a client proposes SET "k" "v" to the leader
    Then the committed history shall include SET "k" "v" on every node
    And the system shall keep all node logs matching

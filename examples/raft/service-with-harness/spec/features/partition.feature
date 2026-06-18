Feature: Behaviour under a network partition (REQ-007)
  Under a partition that isolates a minority, the majority keeps serving writes
  and the minority cannot commit; when the partition heals, the minority catches
  up and no committed write is lost.

  Scenario: REQ-007 — majority serves through a partition and the minority heals
    Given a fresh cluster of 5 nodes
    When the cluster runs until a leader is elected
    And a minority of 2 nodes is partitioned away from the leader
    And the majority commits SET "k" "v"
    Then after healing every node shall hold the committed value "v" for "k"

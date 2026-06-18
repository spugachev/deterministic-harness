Feature: RESP command parser (REQ-001)
  The parser turns untrusted bytes into a typed command and is total: it never
  panics and rejects malformed input with an error instead.

  Scenario: REQ-001 — a well-formed inline GET parses
    Given the raw bytes GET foo
    When the parser runs
    Then the system shall return a GET command

  Scenario: REQ-001 — a RESP array SET parses
    Given the raw bytes *3\r\n$3\r\nSET\r\n$1\r\nk\r\n$1\r\nv\r\n
    When the parser runs
    Then the system shall return a SET command

  Scenario: REQ-001 — an unknown verb is rejected without panicking
    Given the raw bytes INCR foo
    When the parser runs
    Then the system shall reject the input without panicking

  Scenario: REQ-001 — empty input is rejected without panicking
    Given the raw bytes EMPTY
    When the parser runs
    Then the system shall reject the input without panicking

  Scenario: REQ-001 — a truncated RESP array is rejected without panicking
    Given the raw bytes *2\r\n$3\r\nGET\r\n
    When the parser runs
    Then the system shall reject the input without panicking

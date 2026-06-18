Feature: Parse a transfer command from untrusted text (REQ-001)
  The single-line protocol is "TRANSFER <from> <to> <amount> <key>". The parser
  accepts arbitrary input and always returns a typed result — never a panic.

  Scenario: REQ-001 — a well-formed line parses into a typed command
    Given the input line "TRANSFER 1 2 500 abc-key"
    When the line is parsed
    Then parsing shall succeed with from 1, to 2, amount 500 and key "abc-key"

  Scenario: REQ-001 — a non-numeric id is a typed error
    Given the input line "TRANSFER x 2 500 k"
    When the line is parsed
    Then parsing shall fail with a typed error

  Scenario: REQ-001 — too few fields is a typed error
    Given the input line "TRANSFER 1 2 500"
    When the line is parsed
    Then parsing shall fail with a typed error

  Scenario: REQ-001 — an overflowing amount is a typed error not a panic
    Given the input line "TRANSFER 1 2 18446744073709551616 k"
    When the line is parsed
    Then parsing shall fail with a typed error

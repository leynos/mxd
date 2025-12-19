Feature: Transaction streaming framing
  Hotline transactions may be split across multiple fragments. Consumers
  should be able to process each fragment incrementally without buffering the
  entire payload in memory.

  Scenario: Streams a multi-fragment payload incrementally
    Given a fragmented transaction with total size 2097152 across 64 fragments
    When I stream the transaction fragments with a limit of 3145728 bytes
    Then streaming succeeds
    And I receive 64 fragments
    And the total streamed size is 2097152 bytes
    And each fragment is at most 32768 bytes

  Scenario: Rejects mismatched continuation headers during streaming
    Given a fragmented transaction with mismatched continuation headers
    When I stream the transaction fragments with a limit of 1048576 bytes
    Then streaming fails with error "continuation header mismatch"

  Scenario: Rejects payload exceeding the stream limit
    Given a transaction with total size 10 and data size 10
    When I stream the transaction fragments with a limit of 5 bytes
    Then streaming fails with error "payload too large"


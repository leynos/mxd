Feature: Wireframe transaction codec
  The codec reads 20-byte transaction headers and reassembles fragmented
  payloads according to the Hotline protocol specification in docs/protocol.md.

  Scenario: Decodes a single-frame transaction with payload
    Given a transaction with total size 20 and data size 20
    When I decode the transaction frame
    Then decoding succeeds
    And the payload length is 20

  Scenario: Decodes an empty transaction
    Given a transaction with total size 0 and data size 0
    When I decode the transaction frame
    Then decoding succeeds
    And the payload length is 0

  Scenario: Reassembles multi-fragment request
    Given a fragmented transaction with total size 65536 across 3 fragments
    When I decode the transaction frame
    Then decoding succeeds
    And the payload length is 65536

  Scenario: Rejects data size exceeding total size
    Given a transaction with total size 10 and data size 20
    When I decode the transaction frame
    Then decoding fails with "data size exceeds total"

  Scenario: Rejects empty data with non-zero total
    Given a transaction with total size 100 and data size 0
    When I decode the transaction frame
    Then decoding fails with "data size is zero but total size is non-zero"

  Scenario: Rejects invalid flags
    Given a transaction with flags 1
    When I decode the transaction frame
    Then decoding fails with "invalid flags"

  Scenario: Rejects total size exceeding 1 MiB
    Given a transaction with total size 1048577 and data size 32768
    When I decode the transaction frame
    Then decoding fails with "total size exceeds maximum"

  Scenario: Rejects data size exceeding 32 KiB
    Given a transaction with total size 40000 and data size 40000
    When I decode the transaction frame
    Then decoding fails with "data size exceeds maximum"

  Scenario: Rejects mismatched continuation header
    Given a fragmented transaction with mismatched continuation headers
    When I decode the transaction frame
    Then decoding fails with "header mismatch"

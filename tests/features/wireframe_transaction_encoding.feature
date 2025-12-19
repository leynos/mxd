Feature: Wireframe transaction encoding
  The wireframe codec emits the Hotline 20-byte transaction header and payload
  framing in exactly the same bytes as the legacy transaction writer for the
  shared parameter-encoded cases.

  Scenario: Encodes a single-frame parameter transaction
    Given a parameter transaction with 1 field
    When I encode the transaction
    Then encoding succeeds
    And the encoded bytes match the legacy encoder

  Scenario: Encodes an empty-parameter transaction
    Given a parameter transaction with 0 field
    When I encode the transaction
    Then encoding succeeds
    And the encoded bytes match the legacy encoder

  Scenario: Encodes a fragmented parameter transaction
    Given a parameter transaction with a 40000-byte field value
    When I encode the transaction
    Then encoding succeeds
    And the encoded bytes match the legacy encoder
    And the encoded transaction is fragmented into 2 frames

  Scenario: Rejects encoding when the header size does not match the payload
    Given a transaction with mismatched header and payload sizes
    When I encode the transaction
    Then encoding fails with "size mismatch"

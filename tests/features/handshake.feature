Feature: Protocol handshake library contract
  Scenario: Accepts hotline handshake
    Given a handshake buffer with protocol TRTP and version 1
    When the handshake is parsed
    Then the handshake result is accepted

  Scenario: Rejects unexpected protocol magic
    Given a handshake buffer with protocol WRNG and version 1
    When the handshake is parsed
    Then the handshake result is rejected with code 1

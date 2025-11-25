Feature: Wireframe handshake preamble
  Wireframe reads a Hotline handshake before routing transactions. It should
  accept the TRTP header and reject malformed inputs.

  Scenario: Accepts a Hotline handshake
    Given a valid wireframe handshake preamble
    When I decode the wireframe preamble
    Then the wireframe preamble decodes successfully
    And the sub-protocol is "CHAT"
    And the handshake version is 1
    And the handshake sub-version is 7

  Scenario: Rejects handshake with wrong protocol ID
    Given a malformed wireframe preamble with kind "wrong-protocol"
    When I decode the wireframe preamble
    Then decoding fails with "invalid protocol id"

  Scenario: Rejects handshake with unsupported version
    Given a malformed wireframe preamble with kind "unsupported-ver"
    When I decode the wireframe preamble
    Then decoding fails with "unsupported version"

  Scenario: Rejects truncated handshake
    Given a malformed wireframe preamble with kind "truncated"
    When I decode the wireframe preamble
    Then decoding fails with "UnexpectedEnd"

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

  Scenario Outline: Rejects malformed Hotline handshakes
    Given a malformed wireframe preamble with kind "<kind>"
    When I decode the wireframe preamble
    Then decoding fails with "<message>"

    Examples:
      | kind            | message              |
      | wrong-protocol  | invalid protocol id  |
      | unsupported-ver | unsupported version  |
      | truncated       | UnexpectedEnd        |

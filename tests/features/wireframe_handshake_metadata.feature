Feature: Wireframe handshake metadata persistence
  Wireframe should retain the negotiated sub-protocol and sub-version for
  each connection so later handlers can apply compatibility shims.

  Scenario: Stores metadata for valid Hotline handshakes
    Given a wireframe server that records handshake metadata
    When I complete a Hotline handshake with sub-protocol "CHAT" and sub-version 7
    Then the recorded handshake sub-protocol is "CHAT"
    And the recorded handshake sub-version is 7
    And the handshake registry is cleared after teardown

  Scenario: Rejects invalid handshakes without persisting metadata
    Given a wireframe server that records handshake metadata
    When I send a Hotline handshake with protocol "WRNG" and version 1
    Then no handshake metadata is recorded

  Scenario: Metadata does not leak between connections
    Given a wireframe server that records handshake metadata
    When I complete a Hotline handshake with sub-protocol "CHAT" and sub-version 7
    And I complete a Hotline handshake with sub-protocol "NEWS" and sub-version 1
    Then the recorded handshake sub-protocol is "NEWS"
    And the recorded handshake sub-version is 1

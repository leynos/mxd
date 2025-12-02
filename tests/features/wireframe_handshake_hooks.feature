Feature: Wireframe handshake replies
  Wireframe sends Hotline handshake replies for success, errors, and timeouts.

  Scenario: Successful handshake replies OK
    Given a wireframe server handling handshakes
    When I send a valid Hotline handshake
    Then the handshake reply code is 0

  Scenario: Invalid protocol returns an error
    Given a wireframe server handling handshakes
    When I send a Hotline handshake with protocol "WRNG" and version 1
    Then the handshake reply code is 1

  Scenario: Unsupported version returns an error
    Given a wireframe server handling handshakes
    When I send a Hotline handshake with protocol "TRTP" and version 2
    Then the handshake reply code is 2

  Scenario: Idle connection times out during handshake
    Given a wireframe server handling handshakes
    When I connect without sending a handshake
    Then the handshake reply code is 3

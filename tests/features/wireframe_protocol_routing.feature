Feature: Wireframe protocol transaction routing
  The HotlineProtocol adapter routes incoming transactions through the domain
  command dispatcher and returns appropriate replies.

  Background:
    Given a wireframe server with the protocol adapter registered

  Scenario: Protocol adapter registers lifecycle hooks
    When I inspect the server configuration
    Then the protocol adapter is registered

  Scenario: Transaction routing returns error for unparseable frame
    Given a connected client
    When the client sends an invalid transaction frame
    Then the reply indicates an internal error

  Scenario: Transaction routing returns error for unknown command type
    Given a connected client
    When the client sends a transaction with unknown type
    Then the reply indicates an internal error

  Scenario: Unauthenticated client receives error for protected command
    Given a connected client
    When the client sends a get file list command without authentication
    Then the reply indicates a permission error

  Scenario: Error reply preserves transaction id
    Given a connected client
    When the client sends a transaction with id 12345 and unknown type
    Then the reply transaction id is 12345
    And the reply indicates an internal error

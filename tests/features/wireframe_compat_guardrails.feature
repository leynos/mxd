Feature: Wireframe compatibility guardrails
  The WireframeRouter enforces compatibility hooks on every
  routed transaction.

  Scenario: Login with Hotline 1.9 version includes banner fields
    Given a wireframe server with user accounts
    When a Hotline 1.9 client logs in
    Then the login reply includes banner fields

  Scenario: Login with SynHX client omits banner fields
    Given a wireframe server with user accounts
    When a SynHX client logs in
    Then the login reply does not include banner fields

  Scenario: XOR-encoded login succeeds through the router
    Given a wireframe server with user accounts
    When a client sends a XOR-encoded login
    Then the login succeeds
    And XOR encoding is enabled for the connection

  Scenario: File list request succeeds after login through the router
    Given a wireframe server with user accounts
    And a logged-in client
    When the client requests the file name list
    Then the reply contains file names

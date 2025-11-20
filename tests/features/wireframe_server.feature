Feature: Wireframe server bootstrap
  The Wireframe binary resolves configuration before starting listeners so
  operators can detect configuration errors early.

  Scenario: Accepts valid bind addresses
    Given a wireframe configuration binding to "127.0.0.1:0"
    When I bootstrap the wireframe server
    Then bootstrap succeeds
    And the resolved bind address is "127.0.0.1:0"

  Scenario: Rejects invalid bind addresses
    Given a wireframe configuration binding to "invalid-bind"
    When I bootstrap the wireframe server
    Then bootstrap fails with message "invalid bind address"

Feature: create-user command uses shared server library
  Scenario: successful create-user invocation
    Given a temporary sqlite database
    And server configuration bound to that database
    When the operator runs create-user with username "alice" and password "secret"
    Then the command completes successfully
    And the database contains a user named "alice"

  Scenario: create-user rejects missing password
    Given a temporary sqlite database
    And server configuration bound to that database
    When the operator runs create-user with username "bob" and no password
    Then the command fails with message "missing password"

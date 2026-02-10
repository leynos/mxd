Feature: XOR text field compatibility
  XOR-encoding should be detected and handled transparently so legacy clients
  can authenticate and post news without exposing the quirk to the domain.

  Scenario: XOR-encoded login succeeds
    Given a routing context with user accounts
    When I send a login with XOR-encoded credentials
    Then the reply error code is 0
    And XOR compatibility is enabled

  Scenario: XOR-encoded message toggles compatibility
    Given a routing context with user accounts
    When I send an unknown transaction with XOR-encoded message "hello"
    Then the reply error code is 3
    And XOR compatibility is enabled

  Scenario: Plaintext message keeps XOR compatibility disabled
    Given a routing context with user accounts
    When I send an unknown transaction with plaintext message "hello"
    Then the reply error code is 3
    And XOR compatibility is disabled

  Scenario: XOR compatibility remains enabled after plaintext traffic
    Given a routing context with user accounts
    When I send an unknown transaction with XOR-encoded message "hello"
    And I send an unknown transaction with plaintext message "hello"
    Then the reply error code is 3
    And XOR compatibility is enabled

  Scenario: XOR-encoded news article body is accepted
    Given a routing context with news articles
    When I post a news article with XOR-encoded fields
    Then the reply error code is 0
    And XOR compatibility is enabled

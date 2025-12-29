Feature: Wireframe transaction routing
  The wireframe middleware routes incoming Hotline transactions to domain
  handlers and returns appropriate replies.

  Scenario: Routes unknown transaction type to error handler
    Given a wireframe server handling transactions
    When I send a transaction with unknown type 65535
    Then the reply has error code 1

  Scenario: Routes truncated frame to error handler
    Given a wireframe server handling transactions
    When I send a truncated frame of 10 bytes
    Then the reply has error code 3

  Scenario: Preserves transaction ID in error replies
    Given a wireframe server handling transactions
    When I send a transaction with unknown type 65535 and ID 12345
    Then the reply has transaction ID 12345

  Scenario: Preserves transaction type in error replies
    Given a wireframe server handling transactions
    When I send a transaction with unknown type 65535 and ID 99
    Then the reply has transaction type 65535

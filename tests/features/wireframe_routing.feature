Feature: Wireframe transaction routing
  The wireframe middleware routes incoming Hotline transactions to domain
  handlers and returns appropriate replies.

  Scenario: Routes unknown transaction type to error handler
    Given a wireframe server handling transactions
    When I send a transaction with unknown type 65535
    Then the reply has error code 3

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

  Scenario: Login succeeds
    Given a routing context with user accounts
    When I send a login transaction for "alice" with password "secret"
    Then the reply has error code 0
    And the session is authenticated

  Scenario: File listing returns permitted files
    Given a routing context with file access entries
    And I send a login transaction for "alice" with password "secret"
    When I request the file name list
    Then the reply lists files "fileA.txt" and "fileC.txt"

  Scenario: News categories are listed at the root
    Given a routing context with news categories
    When I request the news category list
    Then the reply lists news categories "Bundle", "General", and "Updates"

  Scenario: News articles are listed for a category
    Given a routing context with news articles
    When I request the news article list for "General"
    Then the reply lists news articles "First" and "Second"

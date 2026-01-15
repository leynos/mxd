Feature: Session privilege enforcement
  The session tracks authentication status and privileges, enforcing access
  control for privileged operations.

  Background:
    Given a routing context with user accounts and news data

  Scenario: Unauthenticated user receives error for file listing
    Given the session is not authenticated
    When I request the file name list
    Then the reply has error code 1

  Scenario: Authenticated user can list files
    Given I send a login transaction for "alice" with password "secret"
    When I request the file name list
    Then the reply has error code 0

  Scenario: Authenticated but unprivileged user cannot list files
    Given the session is authenticated but unprivileged
    When I request the file name list
    Then the reply has insufficient privileges error

  Scenario: Unauthenticated user receives error for news category listing
    Given the session is not authenticated
    When I request the news category list
    Then the reply has error code 1

  Scenario: Authenticated user can list news categories
    Given I send a login transaction for "alice" with password "secret"
    When I request the news category list
    Then the reply has error code 0

  Scenario: Authenticated but unprivileged user cannot list news categories
    Given the session is authenticated but unprivileged
    When I request the news category list
    Then the reply has insufficient privileges error

  Scenario: Unauthenticated user receives error for posting news
    Given the session is not authenticated
    When I post a news article titled "Test" to "General"
    Then the reply has error code 1

  Scenario: Authenticated user can post news
    Given I send a login transaction for "alice" with password "secret"
    When I post a news article titled "Test" to "General"
    Then the reply has error code 0

  Scenario: Authenticated but unprivileged user cannot post news
    Given the session is authenticated but unprivileged
    When I post a news article titled "Test" to "General"
    Then the reply has insufficient privileges error

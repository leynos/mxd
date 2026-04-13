Feature: File-node repository
  The additive FileNode schema should expose a stable root folder, enforce
  sibling uniqueness, and honour explicit resource grants without touching the
  legacy file tables.

  Scenario: Duplicate top-level names are rejected
    Given a migrated file-node repository
    And a user "alice" exists
    When I create the root child folder "docs" as "alice"
    And I try to create the root child folder "docs" as "alice"
    Then the duplicate insert is rejected

  Scenario: Explicit resource grants filter visible root children
    Given a migrated file-node repository
    And a user "alice" exists
    And a user "bob" exists
    And a root file "shared.txt" created by "alice"
    And a root file "private.txt" created by "alice"
    And "bob" has download access to "shared.txt"
    When I list root children permitted for "bob"
    Then the permitted child names equal "shared.txt"

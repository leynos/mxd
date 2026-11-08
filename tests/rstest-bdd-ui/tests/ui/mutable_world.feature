Feature: Mutable world async step contract

  Scenario: async mutable world
    Given a mutable world
    When the world records a value
    Then the value is recorded

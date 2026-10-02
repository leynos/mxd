Feature: BDD migration contracts
  Scenario: Yielding steps retain fixture overrides
    When migration work suspends before returning a replacement
    Then migration observes replacement 7

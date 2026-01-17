Feature: Outbound messaging

  Scenario: Push to the current connection
    Given a wireframe outbound messenger with a registered connection
    When I push a low priority message to the current connection
    Then the outbound push succeeds
    And the low priority queue receives the message

  Scenario: Missing outbound target
    Given a wireframe outbound messenger without a registered connection
    When I push a low priority message to the current connection
    Then the outbound push fails with "outbound target unavailable"

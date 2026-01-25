Feature: Session gating verification model
  The session gating Stateright model verifies that authentication and
  privilege enforcement remain intact under reordering.

  Background:
    Given the session gating model uses default bounds

  Scenario: Default model verifies session gating properties
    When I verify the session gating model
    Then the verification completes
    And the properties are satisfied

  Scenario: Default model explores a non-trivial state space
    When I verify the session gating model
    Then the model explores at least 100 states

  Scenario: Out-of-order delivery is represented
    Then the model includes the out-of-order delivery property

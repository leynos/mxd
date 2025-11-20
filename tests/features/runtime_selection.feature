Feature: Runtime selection
  The server selects its networking runtime using Cargo features so the
  bespoke legacy loop can be disabled once the wireframe adapter is ready.

  Scenario: Legacy networking feature enabled
    Given the runtime selection is computed
    Then the active runtime is "legacy"

  Scenario: Legacy networking feature disabled
    Given the runtime selection is computed
    Then the active runtime is "wireframe"

Feature: Wireframe login compatibility
  Login replies should include banner fields for Hotline 1.8.5 and 1.9, while
  SynHX clients omit them based on handshake sub-version detection.

  Scenario: Hotline 1.8.5 login reply includes banner fields
    Given a routing context with user accounts
    And a handshake sub-version 0
    When I send a login request with client version 151
    Then the login reply includes banner fields

  Scenario: Hotline 1.9 login reply includes banner fields
    Given a routing context with user accounts
    And a handshake sub-version 0
    When I send a login request with client version 190
    Then the login reply includes banner fields

  Scenario: SynHX login reply omits banner fields
    Given a routing context with user accounts
    And a handshake sub-version 2
    When I send a login request with client version 190
    Then the login reply omits banner fields

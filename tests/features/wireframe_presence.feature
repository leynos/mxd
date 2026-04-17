Feature: Wireframe presence notifications
  Logged-in clients should see join, update, and disconnect presence traffic
  without polling.

  Scenario: Login, update, and disconnect notifications reach peers
    Given a wireframe server with two presence test users
    And client "bob-client" is connected and logged in as "bob"
    And client "alice-client" is connected and logged in as "alice"
    Then client "bob-client" receives a notify change user for user 1 with name "alice" and icon 0
    When client "alice-client" requests the user name list
    Then the reply lists online users "alice" and "bob"
    When client "alice-client" updates their user info to name "Alice A." and icon 9
    Then client "bob-client" receives a notify change user for user 1 with name "Alice A." and icon 9
    When client "alice-client" disconnects
    Then client "bob-client" receives a notify delete user for user 1

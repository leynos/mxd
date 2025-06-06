# Database Schema Design for HXD Chat Functionality

The following schema maps the chat workflows described in the protocol to a relational design. It mirrors the style used for the news system and keeps compatibility with SQLite.

---

```mermaid
erDiagram
    users {
        INTEGER id PK
        TEXT    username UNIQUE
        TEXT    password
    }

    chat_rooms {
        INTEGER id PK
        INTEGER creator_id FK
        TEXT    subject
        BOOLEAN is_private
        DATETIME created_at
    }

    chat_participants {
        INTEGER chat_room_id FK
        INTEGER user_id FK
        DATETIME joined_at
        PRIMARY KEY (chat_room_id, user_id)
    }

    chat_messages {
        INTEGER id PK
        INTEGER chat_room_id FK
        INTEGER user_id FK
        DATETIME posted_at
        INTEGER options
        TEXT    text
    }

    chat_invites {
        INTEGER id PK
        INTEGER chat_room_id FK
        INTEGER invited_user_id FK
        INTEGER inviter_user_id FK
        DATETIME created_at
    }

    users       ||--o{ chat_rooms       : creates
    chat_rooms  ||--o{ chat_participants: includes
    chat_rooms  ||--o{ chat_messages   : has
    users       ||--o{ chat_messages   : writes
    chat_rooms  ||--o{ chat_invites    : invites
```

---

```sql
CREATE TABLE chat_rooms (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    creator_id  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    subject     TEXT,
    is_private  BOOLEAN DEFAULT 0,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE chat_participants (
    chat_room_id INTEGER NOT NULL REFERENCES chat_rooms(id) ON DELETE CASCADE,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at    DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (chat_room_id, user_id)
);

CREATE TABLE chat_messages (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_room_id INTEGER NOT NULL REFERENCES chat_rooms(id) ON DELETE CASCADE,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    posted_at    DATETIME DEFAULT CURRENT_TIMESTAMP,
    options      INTEGER DEFAULT 0,
    text         TEXT NOT NULL
);

CREATE TABLE chat_invites (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_room_id    INTEGER NOT NULL REFERENCES chat_rooms(id) ON DELETE CASCADE,
    invited_user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    inviter_user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at      DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_chat_participants_room ON chat_participants(chat_room_id);
CREATE INDEX idx_chat_participants_user ON chat_participants(user_id);
CREATE INDEX idx_chat_messages_room ON chat_messages(chat_room_id);
CREATE INDEX idx_chat_messages_user ON chat_messages(user_id);
CREATE INDEX idx_chat_invites_invited ON chat_invites(invited_user_id);
```

### Why these tables?

* **Chat rooms and messages** – Transactions `Send Chat (105)` and `Chat Message (106)` operate on a room context and carry options bits for alternative styles【F:docs/protocol.md†L284-L306】.
* **Joining and leaving** – `Join Chat (115)` and `Leave Chat (116)` maintain membership lists which map directly to `chat_participants`【F:docs/protocol.md†L309-L338】【F:docs/protocol.md†L339-L351】.
* **Invitations** – Private chats are created and managed through `Invite New Chat (112)` and related transactions (113/114), so `chat_invites` records outstanding invitations and who sent them【F:docs/protocol.md†L364-L407】.
* **Subjects** – `Set Chat Subject (120)` triggers `Notify Chat Subject (119)` to broadcast topic changes, stored in `chat_rooms.subject`【F:docs/protocol.md†L352-L361】.

This schema complements the existing users table and allows efficient retrieval of room participants, chat history, and pending invites while matching the protocol’s workflow.

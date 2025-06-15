# Hotline Server Protocol v1.8.5: Transactions and Functionality Guide

Hotline is a client/server system providing chat, private messaging, file
sharing, and a bulletin-board style news system. Hotline Server v1.8.5
communicates with Hotline clients using a defined set of protocol transactions.
This guide describes **every protocol transaction type** in Hotline 1.8.5 and
explains how each one correlates with server functionality and what end-users
experience. We group transactions by functionality (User Login & Presence, Chat,
Private Messaging, File Services, News, Administration) for clarity. For each
transaction, we provide its **ID and name**, its **purpose**, **initiator**
(client or server), key **parameters** passed and results returned, the
**server’s behavior**, and the **user-visible effect**. (Note: Tracker and HTTP
tunneling-related transactions are omitted as requested.)

## Binary Message Formats (Hotline Protocol v 1.8.5)

All multi-byte integers are transmitted **big-endian (“network byte order”)**.
No padding or alignment bytes are used: the fields follow one another exactly as
listed.

### 1 Session-initialisation handshake

| Offset | Size (bytes) | Field               | Meaning                                                                              |
| -----: | ------------ | ------------------- | ------------------------------------------------------------------------------------ |
| 0      | 4            | **Protocol ID**     | ASCII **“TRTP”** (0x54 52 54 50). Distinguishes Hotline from other TCP services.     |
| 4      | 4            | **Sub-protocol ID** | Application-specific tag (e.g. “CHAT”, “FILE”). Can be 0.                            |
| 8      | 2            | **Version**         | Currently **0x0001**. A server should refuse versions it does not understand.        |
| 10     | 2            | **Sub-version**     | Application-defined; often used for build/revision numbers.                          |

**Direction:** Client → Server. The server replies immediately with:

| Offset | Size | Field       | Meaning                                         |
| -----: | ---- | ----------- | ----------------------------------------------- |
| 0      | 4    | Protocol ID | Must echo “TRTP”.                               |
| 4      | 4    | Error code  | **0 = OK**. Non-zero → connection is dropped.   |

A compliant implementation simply waits for the four-byte error code and aborts
if it is non-zero. No further data follow.

______________________________________________________________________

### 2 General transaction frame

Every request **and** reply after the handshake is wrapped in a fixed-length
header followed by an optional parameter block.

| Offset | Size  | Field          | Notes                                                                                                                                                                                         |
| -----: | ----: | -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0      | 1     | **Flags**      | Reserved – **always 0** for v 1.8.5.                                                                                                                                                          |
| 1      | 1     | **Is-reply**   | **0 = request**, **1 = reply**.                                                                                                                                                               |
| 2      | 2     | **Type**       | Transaction ID (see full list in protocol spec – e.g. 0x006B = *Login*).                                                                                                                      |
| 4      | 4     | **ID**         | Client-chosen non-zero number. Replies **must echo** the same value. Allows out-of-order matching.                                                                                            |
| 8      | 4     | **Error code** | Meaningful **only when Is-reply = 1** (0 = success).                                                                                                                                          |
| 12     | 4     | **Total size** | Entire byte count of the transaction’s parameter block **across all fragments**.                                                                                                              |
| 16     | 4     | **Data size**  | Size of the parameter bytes **in *this* fragment**. If `Data size < Total size`, further fragments with identical header values follow until the accumulated data length equals `Total size`. |

*Header length = 20 bytes.*

______________________________________________________________________

#### 2.1 Parameter list (payload block)

If `Data size > 0`, the parameter bytes begin with:

| Offset | Size | Field           | Meaning                                       |
| -----: | ---- | --------------- | --------------------------------------------- |
| 0      | 2    | **Param-count** | Number of field/value pairs in this fragment. |

Immediately afterwards, *Param-count* **records** follow:

| Offset | Size         | Field      | Meaning                                                                                   |
| -----: | ------------ | ---------- | ----------------------------------------------------------------------------------------- |
| 0      | 2            | Field ID   | See master field-ID table (e.g. 0x0066 = *User Name*).                                    |
| 2      | 2            | Field size | Length of the data portion in bytes.                                                      |
| 4      | *Field size* | Field data | Raw value. Interpretation rules: integer (16- or 32-bit), ASCII string, or opaque binary. |

Field IDs never repeat within a single transaction. Integers are unsigned; the
sender may use 16-bit encoding if the value ≤ 0xFFFF, otherwise 32-bit.

#### 2.2 Date parameters

Several protocol fields store timestamps in an eight‑byte structure, often
called *date objects*. The layout is:

- **Year** – 2 bytes (big-endian)
- **Milliseconds** – 2 bytes (big-endian)
- **Seconds** – 4 bytes (big-endian)

All three values use network byte order.

Seconds and milliseconds indicate the elapsed time since **January 1** of the
given year. For example, a value of 70 seconds and 2,000 milliseconds with year
2010 corresponds to *1 January 2010 00:01:12*. Likewise, 432,010 seconds and
5,000 milliseconds with year 2008 represent *6 January 2008 00:00:15*.

To convert a `SYSTEMTIME` structure to this format, compute the seconds field
as:

```text
MONTH_SECS[wMonth - 1] +
(if (wMonth > 2) and isLeapYear(wYear) { 86400 } else { 0 }) +
(wSecond + 60 * (wMinute + 60 * (wHour + 24 * (wDay - 1))))
```

`MONTH_SECS` is a table containing the cumulative seconds at the start of each
month:

| Month       | Seconds    |
| ----------- | ---------: |
| January     | 0          |
| February    | 2,678,400  |
| March       | 5,097,600  |
| April       | 7,776,000  |
| May         | 10,368,000 |
| June        | 13,046,400 |
| July        | 15,638,400 |
| August      | 18,316,800 |
| September   | 20,995,200 |
| October     | 23,587,200 |
| November    | 26,265,600 |
| December    | 28,857,600 |

The year and milliseconds fields are copied directly from the original timestamp
structure.

______________________________________________________________________

### 3 Fragmentation rules

- **Single-part transaction:** `Total size == Data size`; one frame only.
- **Multi-part transaction:** the first fragment carries part 0. Subsequent
  fragments repeat the same header (with appropriate `Data size`) and deliver
  the remaining bytes until the sum of `Data size` values equals `Total size`.
  The parameter list header (2-byte *Param-count*) appears **only in the first
  fragment**. The receiver concatenates payloads before decoding parameters.
- `ID` and `Type` never change across fragments; only the `Data size` field
  differs.

______________________________________________________________________

### 4 Reply semantics

- A reply **must** keep the original `ID`, set **Is-reply = 1**, and fill
  `Error code`.
- If `Error code ≠ 0`, no parameter list is required; many implementations send
  an empty payload (`Total size = Data size = 0`).
- Where a command has no meaningful return data, the server often omits the
  parameter block entirely.

______________________________________________________________________

### 5 Worked example (Login)

| Field    | Hex value   | Comment                  |
| -------- | ----------- | ------------------------ |
| Flags    | 00          | —                        |
| Is-reply | 00          | Request                  |
| Type     | 00 6B       | 107 decimal – *Login*    |
| ID       | 00 00 01 27 | Arbitrary (295)          |
| Error    | 00 00 00 00 | Always zero in a request |
| Total    | 00 00 00 14 | 20 bytes of parameters   |
| Data     | 00 00 00 14 | All in one fragment      |

Parameter block:

| Field ID             | Size | Data (hex)     | Meaning      |
| -------------------: | ---: | -------------- | ------------ |
| 0069 (User Login)    | 0005 | 61 64 6D 69 6E | “admin”      |
| 006A (User Password) | 0000 | —              | empty        |
| 00A0 (Version)       | 0002 | 00 97          | 0x0097 (151) |

Total frame length: 40 bytes. The server’s reply echoes `ID = 0x00000127`, sets
**Is-reply = 1**, inserts its own parameter list, and may set `Error code` if
the login fails.

______________________________________________________________________

### 6 Implementation checklist

1. **Validate before use** – always verify the Protocol ID, `Flags == 0`, and
   sensible sizes.
2. **Allocate big-endian helpers** – it is safer to write explicit *readUInt16BE
   / readUInt32BE* helpers than rely on struct packing.
3. **Never reuse transaction IDs** until a matching reply arrives or the
   connection is closed.
4. **Graceful error path** – on any framing or length anomaly, send a single
   *Disconnect Message* (111) if possible, then drop the socket.
5. **Unit-test fragmentation** – many clients send large parameter blocks (e.g.
   *Upload Folder*) in dozens of fragments.

This guide gives you the precise binary envelope you must generate and parse;
pair it with the full transaction catalogue to complete your Hotline server
implementation.

## User Login and Presence

### Session Handshake (Before Transactions Begin)

When a client first connects to a Hotline server, a handshake occurs to verify
protocol versions. The client sends a 12-byte handshake with a protocol ID
`'TRTP'` and version info, and the server replies with `'TRTP'` and an error
code (0 for success). If the versions or requirements don’t match, the
connection is closed (the user cannot log in). This handshake is not a numbered
transaction but sets up the session for subsequent transactions.

### Login (Transaction 107) – Client Initiates

**ID 107 – Login** (`myTran_Login`) is the first real transaction of a user
login sequence. The client initiates it to authenticate and begin login.
**Purpose:** To log the user into the server with provided credentials and
negotiate client/server capabilities. **Initiator:** Client.

- **Parameters (Client→Server):** The login request includes the user’s login
  name (field 105), password (field 106), and client version number (field 160).
  The version field helps the server handle compatibility (Hotline 1.8.5 uses
  version code 151).
- **Response (Server→Client):** The server validates the credentials. If
  successful, it replies with its own version number (field 160). For Hotline
  1.8.5 (version ≥151), the server also sends additional info: a **community
  banner ID** (field 161) and the server’s name (field 162). These are used by
  the client to fetch and display a banner or server name in the UI. If login
  fails (bad password or user not allowed), the server will **deny the login** –
  typically by closing the connection or sending a disconnect message with an
  error reason (see “Disconnect Message” below), so the user sees a login
  failure message.

**Server behavior:** On receiving a Login request, the server checks the
username/password against its user accounts. If the user is permitted (and not
banned or already online if only one session allowed), the server allocates a
session. It includes version info in the reply and prepares any welcome
materials (like an agreement or banner). If the login is refused (wrong
credentials or banned), the server can either drop the connection or send an
error/disconnect notice. (The protocol defines a generic **Error (100)**
transaction, which may carry an error code or text, but in practice Hotline
servers often simply disconnect with a reason string.)

**End-user experience:** The user enters their login and password in the Hotline
client. If these are correct, the client proceeds to the next login steps
(agreement or entering the server). If incorrect, the user sees “Incorrect login
or password” (the connection closes or an error message is shown). If the server
was full or the user banned, a message like “Server full” or “You have been
banned” may be displayed (the server likely uses a disconnect message with that
text).

### Server Agreement (Transaction 109) – Server Initiates

**ID 109 – Show Agreement** (`myTran_ShowAgreement`) is sent by the server
**immediately after a successful login** if the server requires the user to
accept an agreement (terms of service or welcome message). **Purpose:** To
present the server’s user agreement and banner info to the client as part of
login. **Initiator:** Server (sent to client after login, before finalizing
login).

- **Parameters (Server→Client):** The server sends the agreement text string
  (field 101, “Data”) which the client should display (e.g. in a popup). It also
  indicates if no agreement is available (field 154 set to 1 if there is no
  agreement text). Additionally, the server provides banner information: a
  banner type code (field 152) and either a URL to fetch the banner (field 153,
  if type indicates URL) or the banner image data itself (field 151, if type
  indicates an image was included in-line). The banner type could be “URL” or an
  image format code (e.g. JPEG, GIF).
- **Response:** No reply is expected from the client for this transaction.
  Instead, the client will either display the agreement to the user and wait for
  acceptance, or skip it if `No server agreement` was indicated.

**Server behavior:** The server pauses the login process and waits for the
user’s response. If there is an agreement text configured on the server, it uses
this transaction to send it. The user’s client will typically not let them fully
join until they accept. The banner info tells the client how to load a server
banner (e.g. an image or advertisement) to show in the client UI.

**End-user experience:** Right after logging in, the user is presented with the
server’s agreement or welcome notice (if the server has one). They must click
“Agree/Accept” to proceed. They might also see a banner graphic or link (for
example, the server’s logo or ad) displayed in the client. If no agreement is
required, this step is skipped (field 154=1 was sent, meaning no agreement).

### Agreement Acceptance (Transaction 121) – Client Initiates

**ID 121 – Agreed** (`myTran_Agreed`) is sent by the client after the user
accepts the server’s agreement. **Purpose:** To inform the server that the user
agreed to the terms and to provide the user’s chosen settings (like nickname and
status flags) for the session. **Initiator:** Client.

- **Parameters (Client→Server):** The client includes the user’s desired display
  name (field 102) and chosen user icon ID (field 104). It also sends an
  **Options bitmap** (field 113) representing the user’s preference flags: for
  example, bit value 1 means “Refuse private messages”, 2 means “Refuse private
  chat invites”, and 4 means “Automatic response enabled”. If the user enabled
  an auto-reply message, the client also sends that auto-response text (field
  215\) in the same request.
- **Response:** The server doesn’t send a direct reply to `Agreed` (no reply
  expected). Instead, upon receiving this, the server finalizes the login.

**Server behavior:** Once the server gets the Agreed transaction, it knows the
user accepted the terms and is ready to fully join. The server records the
user’s chosen nickname, icon, and privacy preferences (refusal flags or
auto-reply message). At this point, the user is officially “online” on the
server. The server will typically respond by sending the user their access
privileges and populating their view of the server (user list, etc.).

- Immediately after this, the server sends a **User Access (354)** transaction
  to the client (described below) to inform them of their privilege level on the
  server.
- The server also considers the user part of the active user list now, and will
  notify other connected users of this new user (via **Notify Change User
  (301)**, described shortly).

**End-user experience:** After clicking “Agree”, the user’s chosen nickname and
icon are set, and their client transitions into the server’s online environment.
They can now see the server contents (like the user list, chat, files, etc.
depending on privileges). The client might update some UI based on preferences
(e.g. mark them as not accepting private chats/messages if those were chosen).

### User Access Privileges (Transaction 354) – Server Initiates

**ID 354 – User Access** (`myTran_UserAccess`) is sent by the server typically
right after login (after agreement). **Purpose:** To inform the client of the
access privileges of the current user’s account. **Initiator:** Server.

- **Parameters (Server→Client):** The server sends the user’s access rights as a
  bitmap (field 110 “User access”). See
  [Access Privilege Bits](#access-privilege-bits-field-110) for what each bit
  represents. No other fields are sent.
- **Response:** The client does not reply to this (one-way notification).

**Server behavior:** The server looks up the privileges associated with the
user’s account (e.g. whether they are an admin or a normal user and what
permissions they have). It then sends this transaction so the client knows what
the user can or cannot do on the server. This could include general privileges
(like “Send Chat”, “Download Files”, “Create Folders”) and possibly
folder-specific or news-specific rights.

**End-user experience:** This transaction itself isn’t visible to the user, but
its effects are. The Hotline client will enable or disable interface features
based on the privileges. For example, if the user does not have “Upload File”
privilege, the upload button might be grayed out; if they have admin rights,
admin functions become available. Essentially, the client tailors the UI and
available actions according to what the server allowed, immediately after login.

#### Access Privilege Bits (Field 110)

The access bitmap sent in field 110 consists of individual privilege bits. These
bits fall into three categories: *general* (user-level), *folder* (per-folder)
and *bundle* (logical grouping). The meaning of each bit is listed below so
implementations can translate the bitmap into permissions:

| Bit  | Privilege              |
| ---: | ---------------------- |
| 0    | Delete File            |
| 1    | Upload File            |
| 2    | Download File          |
| 3    | Rename File            |
| 4    | Move File              |
| 5    | Create Folder          |
| 6    | Delete Folder          |
| 7    | Rename Folder          |
| 8    | Move Folder            |
| 9    | Read Chat              |
| 10   | Send Chat              |
| 11   | Open Chat              |
| 12   | Close Chat             |
| 13   | Show in List           |
| 14   | Create User            |
| 15   | Delete User            |
| 16   | Open User              |
| 17   | Modify User            |
| 18   | Change Own Password    |
| 19   | Send Private Message   |
| 20   | News Read Article      |
| 21   | News Post Article      |
| 22   | Disconnect User        |
| 23   | Cannot be Disconnected |
| 24   | Get Client Info        |
| 25   | Upload Anywhere        |
| 26   | Any Name               |
| 27   | No Agreement           |
| 28   | Set File Comment       |
| 29   | Set Folder Comment     |
| 30   | View Drop Boxes        |
| 31   | Make Alias             |
| 32   | Broadcast              |
| 33   | News Delete Article    |
| 34   | News Create Category   |
| 35   | News Delete Category   |
| 36   | News Create Folder     |
| 37   | News Delete Folder     |

### Retrieving the User List (Transaction 300) – Client Initiates

**ID 300 – Get User Name List** (`myTran_GetUserNameList`) is usually the next
step in the login sequence. After agreeing and receiving privileges, the client
requests the list of currently online users. **Purpose:** To get the roster of
all users connected to the server (for display in the client’s user list).
**Initiator:** Client.

- **Parameters (Client→Server):** No fields are required in the request (the
  server knows to return the full list).
- **Response (Server→Client):** The server replies with one or more **User Name
  with Info** entries (field 300, repeated). Each such field is a data structure
  containing a user’s information: typically their user ID, nickname, icon,
  status flags (like admin or away), etc. (The protocol documentation defines
  the exact structure of “User name with info”.) Essentially, the server dumps
  the current user list.

**Server behavior:** On `GetUserNameList`, the server compiles all connected
users’ info. This includes the user IDs (each user gets a unique ID number for
this session), their nickname, icon ID, and a flags field that indicates things
like whether the user is an admin or has “do not disturb” enabled. It sends all
that in a single reply. This snapshot is used by the client to populate the user
list UI.

**End-user experience:** The user sees the “user list” panel populate with all
nicknames currently online. Icons or special markers may denote certain statuses
(for example, an icon might show if a user is an admin or if they refuse private
messages – the client uses the flags from the server to display such icons). At
this point, the new user can see who else is online.

### User Appearance and Updates (Transaction 301 & 302) – Server Initiates

Once the user is online, the server keeps everyone’s user list updated via
notifications:

- **ID 301 – Notify Change User** (`myTran_NotifyChangeUser`) is sent by the
  server to all clients when a user’s information changes or a new user joins.
  **Purpose:** To add a new user to the list or update an existing user’s data.
  **Initiator:** Server. The server uses this whenever *some other user* has
  either connected or changed their profile.

  - **Parameters (Server→Client):** It includes the affected user’s ID (field
    103), new icon ID (104), flags (112), and name (102). If a new user
    connected, these fields describe the new user; if an existing user changed
    something (like their nickname or status), the fields contain the updated
    info.
  - **Response:** No reply (clients just update their UI).

  **Server behavior:** For a new login, right after adding the user, the server
  broadcasts a Notify Change User to everyone *except* that new user (who got
  the list via 300). This tells other clients “User X has joined” and provides
  their details. Similarly, if a user uses the **Set Client User Info (304)**
  transaction to change their nickname, icon, or preference flags (explained
  next), the server sends Notify Change User to update all clients’ lists with
  the new nickname/icon/status. In v1.8.x, the server also applies this update
  to any chat rooms that the user is in (meaning the change is reflected in chat
  participant lists too).

  **End-user experience:** Other users see one of two things: if a new user
  connected, they appear in the user list (often with a join message like “User
  [name] has connected” in the client’s status area). If an existing user
  changed their name or icon, the list entry updates (their name changes in
  real-time, icon updates, and an indicator might flash that the user updated
  their info). If the user toggled “Refuse chat” or “Refuse messages” or set an
  auto-reply, those corresponding icons (like a “no messages” icon) might appear
  or disappear next to the name.

- **ID 302 – Notify Delete User** (`myTran_NotifyDeleteUser`) is sent when a
  user disconnects. **Purpose:** To remove a user from the user list.
  **Initiator:** Server.

  - **Parameters:** The server sends the departing user’s ID (field 103).
  - **Response:** None (clients will remove the user from their list).

  **Server behavior:** As soon as a user disconnects (whether by their own
  action or being kicked), the server broadcasts a Notify Delete User to all
  remaining clients. It identifies which user ID is gone so clients can remove
  it from the roster.

  **End-user experience:** All users see that person disappear from the user
  list. The client typically also shows a message like “User [name] has
  disconnected.” Any open private chats with that user would close or indicate
  that the user left.

### Viewing User Info (Transaction 303) – Client Initiates

**ID 303 – Get Client Info Text** (`myTran_GetClientInfoText`) is used when one
user requests information about another user. **Purpose:** To retrieve a bit of
profile or info text about a specific user. **Initiator:** Client (any client
with permission can do this, typically by selecting “Get Info” on a user).

- **Parameters (Client→Server):** The requesting client sends the target user’s
  ID (field 103) in the request. (This action requires the *Get Client Info*
  privilege (privilege code 24) – by default, regular users likely have it, so
  they can view each other’s info.)
- **Response (Server→Client):** The server replies with the user’s name (field
  102\) and an info text string (field 101, “Data”). The “info text” is a short
  text that the user or admin configured (for example, a real name, contact
  info, or any note). If no info text is set, it could be blank.

**Server behavior:** The server looks up the requested user’s account or session
info. It fetches the “user info” field associated with that user. On Hotline
servers, each account can have an info or real name field, and users themselves
can’t change it in 1.8.5 (there is no direct user command to set a profile text
aside from auto-reply). Usually, this might contain the full name or a comment
an admin set. The server returns that text to the requester.

**End-user experience:** When a user selects “Get Info” on someone, a small info
window opens showing that user’s nickname and their info text. For example, it
might show “User: JohnDoe – Info: John Doe from Support Team.” If the user has
no info text, it might just show their name. This is read-only to the viewer.

### Changing User Settings (Transaction 304) – Client Initiates

**ID 304 – Set Client User Info** (`myTran_SetClientUserInfo`) is how a
connected user updates their own profile/preferences on the server. **Purpose:**
To change the user’s visible name, icon, or privacy options on the server.
**Initiator:** Client (only for the current user themselves).

- **Parameters (Client→Server):** The client sends any of the following that are
  being changed: new user name (field 102) to change their display nickname, new
  icon ID (field 104) to change their icon, and an updated Options bitmap (field
  113\) to change their privacy settings. The Options bits are the same as during
  login: bit 1 = refuse private messages, bit 2 = refuse private chat invites,
  bit 4 = enable automatic response. If bit 4 (auto-response) is set, an
  Automatic Response string (field 215) can be provided with the message to
  auto-send. The client includes only the fields it wants to change (e.g. to
  just toggle “no private messages”, it might send just the options field).
- **Response:** There is no direct reply expected. The server will acknowledge
  the changes implicitly by broadcasting an update to others.

**Server behavior:** When the server receives SetClientUserInfo, it updates that
user’s info in memory: their name (for the session), icon, and flags. It then
notifies all clients (including in any chats) via **Notify Change User (301)**
so that everyone sees the new name/icon/status. This is essentially how nickname
changes or “do not disturb” status is propagated. The server does not persist
these changes to the account database (except possibly the user icon and name
might be session-only if the account name is fixed; in Hotline the nickname can
be different from login and can change per session). Automatic response text is
stored so the server (or the client) can use it if someone messages this user.

**End-user experience:** When the user changes their nickname or icon, they
immediately see it change in their own client. Everyone else online sees the
user’s entry update (the new name appears in the user list, possibly with a
notification). If the user toggles “Refuse Private Messages” or “Refuse Chat”,
an icon (like a no-entry sign) might appear next to their name in others’ user
lists indicating they are not open to PMs or chat invites. If they set an
auto-reply, others won’t see that directly, but it will be sent back when
someone tries to message them (as we’ll see in **Private Messaging** below).

**Summary:** At this point in the login flow, the user is fully online, can see
others, and others see them. The client has possibly also automatically issued
either a **Get File List (200)** or **Get News Category List (370)** request,
depending on user preferences, to load either the file browser or news as the
default view. (Hotline clients let users choose whether to start in the Files or
News section after login.) The next sections describe how file browsing and news
retrieval work, as well as chat and messaging.

## Chat (Public and Private Chat Rooms)

Hotline servers support a main public chat room and additional private chat
rooms. Chat messages are broadcast to all users in the same chat room. The
following transactions handle sending and receiving chat text, joining/leaving
chats, and managing chat invitations and subjects.

### Sending a Chat Message (Transaction 105) – Client Initiates

**ID 105 – Send Chat** (`myTran_ChatSend`) is used whenever a user sends a
message to a chat room. **Purpose:** To transmit a chat message (text line) from
a client to the server (to be distributed to all chat participants).
**Initiator:** Client.

- **Parameters (Client→Server):** The client provides the chat message text
  (field 101, containing the message string). It also may include a **Chat ID**
  (field 114) if the message is intended for a specific chat room other than the
  main/default chat. If no Chat ID is given, it implies the main public chat.
  There’s also an optional “Chat options” field (109) which can indicate a
  “normal” or “alternate” chat message. (In practice, this might differentiate
  between standard chat lines vs. actions or colored text, depending on the
  client UI. By default 0 = normal text, 1 = alternate style.)
- **Response:** The server does not send a direct reply to the sender. Instead,
  the server will echo/distribute the message to all appropriate users via the
  **Chat Message** transaction.

**Server behavior:** Upon receiving a ChatSend from a user, the server checks
that the user has the privilege to send chat (the user needs *Send Chat*
privilege). If yes, the server takes the text and prepares a **Chat Message
(106)** to all users in that chat room (including the sender, typically, so they
see their message appear). If the user is not in the specified chat (e.g. they
haven’t joined a private chat they’re trying to send to), the server might
ignore the message or send an error. If the server has any chat moderation, it
could filter or format the message (but normally it just relays it).

**End-user experience:** The user types a line in the chat input and hits send.
Immediately (round-trip to server), the message appears in the chat window for
them and everyone else present. The client will display it with the user’s name
or icon next to it. If there was a problem (for instance, user muted or no
privilege), the message wouldn’t show and the user might get a notice that they
cannot speak in chat.

### Receiving a Chat Message (Transaction 106) – Server Initiates

**ID 106 – Chat Message** (`myTran_ChatMsg`) is how the server delivers a chat
line to clients in a chat room. **Purpose:** To broadcast a chat message to chat
participants. **Initiator:** Server (sent to each client in the chat).

- **Parameters (Server→Client):** The server includes the chat identifier (field
  114\) to specify which chat room the message belongs to, and the chat text
  (field 101) which is the content of the message. If the Chat ID is not
  provided in the message (perhaps in older contexts or main chat), the protocol
  notes that the Data field might contain a “special chat message” – possibly
  system messages or actions.
- **Response:** Clients do not reply to chat messages.

**Server behavior:** For every incoming chat message, the server sends out a
ChatMsg transaction to each user currently in that chat room (except possibly
the sender if the client is designed to echo locally, but generally the sender
gets it too for consistency). The server may also generate ChatMsg on its own
for server notices (for example, “Admin has muted the room” could be sent as a
chat message by the server software). If chat rooms have IDs (the main chat
might be ID 0 or omitted), the server ensures to tag the message with the
correct chat context so clients only display it in the correct window.

**End-user experience:** The user sees new chat lines appearing in the chat
window. Each ChatMsg usually appears with the sender’s name (the client knows
user ID to name mapping from the user list). If the message was a special type
(e.g. an emote or system message), it might appear in italic or a different
style as handled by the client. All participants in the chat see the same
messages at roughly the same time.

### Joining a Chat Room (Transaction 115) – Client Initiates

**ID 115 – Join Chat** (`myTran_JoinChat`) is used when a client wants to enter
a chat room (either the main chat or a private chat). **Purpose:** To join an
existing chat channel and retrieve its current state (participants and topic).
**Initiator:** Client.

- **Parameters (Client→Server):** The client sends the Chat ID of the chat it
  wishes to join (field 114). For the main public chat, there may be a
  well-known ID (often 0 or omitted; Hotline main chat might not require an
  explicit join in some implementations, but if it does, the ID would be known
  or obtained via an invite mechanism).
- **Response (Server→Client):** The server replies with the current **Chat
  subject** (field 115) – the topic/title of that chat room – and a list of
  current users in the chat. The user list is given by one or more **User name
  with info** entries (field 300, repeated) listing each user currently in that
  chat. This is similar to the main user list but scoped to that chat room.

**Server behavior:** When a user requests to join a chat, the server checks if
the chat exists and if the user is allowed (if it’s a private chat, perhaps an
invite is required or a password, but Hotline’s private chats are usually by
invitation only). Assuming they have access (for main chat, if they have *Read
Chat* privilege they can join; for private, if they were invited or it’s open),
the server adds that user to the chat’s participant list. It then sends back the
chat’s current subject (topic) and all users currently there. The server also
notifies *other members of that chat* that this user has joined via a **Notify
Chat Change User (117)** transaction (described next).

**End-user experience:** When the user opens a chat window (main or a private
chat), the client sends JoinChat. The user then sees the chat room interface
populate: the chat topic (subject) is displayed (e.g. “Topic: General
Discussion”), and the list of users currently in that chat appears (often on the
side of the chat window). They can now participate and see messages in that
chat. Other people in the room will see a notification that this user has joined
(often as a system message like “[User] has joined the chat”).

### Leaving a Chat Room (Transaction 116) – Client Initiates

**ID 116 – Leave Chat** (`myTran_LeaveChat`) is sent when a user leaves a chat
room. **Purpose:** To exit a chat and inform the server (so the server can
update the participant list). **Initiator:** Client.

- **Parameters:** The client specifies the Chat ID of the chat to leave (field
  114).
- **Response:** None expected directly.

**Server behavior:** The server removes the user from that chat’s participant
list. It then sends a **Notify Chat Delete User (118)** to the remaining
participants to tell them this user left. The server might also clear any state
for that chat for the user (they will no longer receive messages from it).

**End-user experience:** The user closing a chat window or leaving a chat causes
them to no longer see messages from that chat. On their side, the chat window
closes. The other users in the chat see that “[User] has left the chat” and that
user’s name disappears from the chat’s user list.

### Chat Room Notifications (Transactions 117 & 118) – Server Initiates

These keep track of who is in the chat:

- **ID 117 – Notify Chat Change User** (`myTran_NotifyChatChangeUser`) is sent
  by the server to users in a chat room when *someone joins the chat or changes
  their info while in that chat*. **Purpose:** To add a user to the chat’s user
  list or update their icon/name in that chat. **Initiator:** Server.

  - **Parameters:** Includes the Chat ID (field 114), the user’s ID (103), icon
    ID (104), flags (112), and name (102). Essentially the same data as a 301
    NotifyChangeUser, but scoped to a chat.
  - **Response:** None.

  **Server behavior:** When a user joins, the server sends this to existing
  members so the new user appears in their chat roster. If a user already in the
  chat changes their nickname or icon (via 304), the global NotifyChangeUser
  (301) is sent to all clients, and **Hotline 1.8.x applies that update to chat
  rooms as well**. So 117 is actually mainly used for the join event (the
  documentation notes in 1.8.x it’s effectively only used for new joiners, since
  general info changes are covered by 301 in all contexts).

  **End-user experience:** In the chat’s user list pane, a new name appears when
  someone joins. If someone already in the chat changed their name/icon, it
  updates there too (often instant since 301 handles it globally). The chat text
  area might also show a system line “User [Name] has joined the chat.” This
  makes it clear who just came in.

- **ID 118 – Notify Chat Delete User** (`myTran_NotifyChatDeleteUser`) is sent
  when someone leaves a chat. **Purpose:** To remove a user from the chat’s
  participant list. **Initiator:** Server.

  - **Parameters:** Chat ID (114) and User ID (103) of the user who left.
  - **Response:** None.

  **Server behavior:** On a user’s departure (or if they are kicked from the
  chat), the server sends this to remaining chat members. They will remove that
  user’s name from the list.

  **End-user experience:** The users in the chat see that user disappear from
  the user list. Often a message like “User [Name] has left the chat” is shown.
  The chat continues for others; the leaving user is no longer in that room (and
  if it was a private chat and they were the last to leave, the chat room might
  cease to exist entirely).

### Chat Invitations (Transactions 112, 113, 114) – Private Chats

Hotline allows private chats: one user can invite others into a new chat room.
These transactions handle inviting and accepting/declining chat invitations.

- **ID 112 – Invite to New Chat** (`myTran_InviteNewChat`) is used to start a
  brand new private chat with one or more users. **Purpose:** Client A invites
  selected users to a new chat room. **Initiator:** Client.

  - **Parameters (Client→Server):** It can include one or multiple user IDs to
    invite. Field 103 holds a User ID, and it can be repeated (103, 103, …
    multiple times) to invite several people at once. If no user ID is given, it
    likely means invite none (which wouldn’t make sense, so usually at least
    one).
  - **Response (Server→Client):** The server responds with a set of fields that
    effectively *create the new chat*: it returns the list of initial
    participants and the new chat’s ID. Specifically, the reply includes the
    User ID (103), User icon ID (104), User flags (112), and User name (102) for
    **each user invited who joins**, as well as the new Chat ID (114) for this
    chat room. Essentially the server is confirming the chat creation and
    listing who is in it (likely echoing back the inviter and any invitees who
    are immediately in the chat, which initially might be just the inviter until
    others accept).

  **Server behavior:** When a user invites others to a new chat, the server
  creates a new chat room (assigning a unique Chat ID). The server adds the
  inviter to it immediately (they initiated it, so they are in that chat by
  default). It then sends out invitations to each user specified via **Invite to
  Chat (113)** transactions (see below). The reply to the inviter contains the
  Chat ID and currently present users (initially just themselves). As invitees
  join, the inviter will receive NotifyChatChangeUser events as those people
  come in.

  **End-user experience:** The inviting user will see a new chat window open (or
  a prompt) representing the private chat. Initially it might just show
  themselves until others join. The chat’s participant list is populated with
  those who accept. The invitee(s) on their side will receive an invite
  notification.

- **ID 113 – Invite to Chat** (`myTran_InviteToChat`) covers two cases: the
  server sending an invite to a user, and a client inviting a user to an
  *existing* chat room.

  - **Client-initiated (optional case):** A client already in a chat can invite
    another user to *that existing chat* (not creating a new one). In that case,
    **Initiator: Client**, and the client sends the User ID to invite (103) and
    the Chat ID of the chat room (114). There is no reply expected from the
    server for this form. The server will then send an invite notification to
    the target user (using the server form below).

  - **Server-initiated (common case for private chat invites):** **Initiator:
    Server.** The server sends this to a user to notify them that they are
    invited to join a chat.

    - **Parameters (Server→Client):** The server provides the Chat ID (114) and
      the ID of the user who invited them (103), plus that inviter’s name (102).
      This lets the receiving client know “User [Name] is inviting you to Chat
      [ID]”. The client typically will prompt “So-and-so invites you to a
      private chat. Accept?”.
    - **Response:** The invited user does not reply to the InviteToChat
      directly. They will either join (via sending `Join Chat (115)`) or reject
      (via `Reject Chat Invite (114)` transaction).

  **Server behavior:** For a new chat (triggered by 112) or an existing chat
  invite, the server sends InviteToChat to each targeted user. If the server
  version is older (\<1.5) and the invitee had auto-response or reject flags
  set, the doc notes the client will auto-send a RejectChatInvite (114)
  immediately without prompting. In modern case, the client likely prompts the
  user unless “auto-refuse chat invites” is enabled, in which case it
  automatically declines. The server awaits either a join or reject from each
  invitee.

  **End-user experience:** The user receiving this sees a pop-up or
  notification: “User X invites you to a private chat.” They can choose to
  accept or decline. If their client is set to auto-refuse chats (do not
  disturb), they might not even be prompted; it will auto-decline on their
  behalf, possibly with an auto message if set.

- **ID 114 – Reject Chat Invite** (`myTran_RejectChatInvite`) is sent by a
  client to refuse a chat invitation. **Purpose:** To let the server (and
  indirectly the inviter) know that the invitation was declined. **Initiator:**
  Client (invitee).

  - **Parameters:** Chat ID (114) of the invite being rejected. That’s all
    that’s needed (the server knows who is rejecting by the session).
  - **Response:** None.

  **Server behavior:** On receiving a rejection, the server will not add that
  user to the chat. It may inform the inviter in some fashion (Hotline may send
  the inviter a system message like “User X declined the chat invite” – likely
  implemented as a **Server Message** to the inviter with a flag indicating
  refusal, or simply not adding the user, which implicitly shows they aren’t
  coming). The chat room, if empty (e.g. everyone declined), might be destroyed.
  There isn’t a specific protocol message to the inviter for rejection in the
  spec except possibly a server message with an option flag for admin vs user
  message.

  **End-user experience:** The inviter sees that the person didn’t join –
  possibly they get a message “[User] refused to join” or they simply never show
  up in the chat. The invitee simply closes the invite dialog or clicks “No,”
  and nothing more happens – they remain out of that chat.

### Chat Subject Management (Transactions 119 & 120)

Chat rooms in Hotline have a “subject” or topic line that can be set, usually
visible at the top of the chat window.

- **ID 120 – Set Chat Subject** (`myTran_SetChatSubject`) is used to change a
  chat’s topic. **Purpose:** To allow a user (often the admin or the one who
  created the chat) to set a new subject line for the chat room. **Initiator:**
  Client.

  - **Parameters (Client→Server):** The client sends the Chat ID (field 114) of
    the room and the new subject string (field 115). The subject text is
    typically a short string.
  - **Response:** No reply expected (the server will notify others).

  **Server behavior:** The server updates the chat room’s subject in its state.
  It then sends a **Notify Chat Subject (119)** to all participants to broadcast
  the new topic.

  **End-user experience:** The user setting the topic might see their chat
  window update immediately. All users in the chat will see the topic change
  (often the client displays something like "Chat topic is now: `new subject`").
  The topic field at the top of the chat window updates for everyone.

- **ID 119 – Notify Chat Subject** (`myTran_NotifyChatSubject`) is the server’s
  message to all chat members that the subject changed. **Purpose:** To
  distribute the new chat topic. **Initiator:** Server.

  - **Parameters:** Chat ID (114) and the new Subject string (115).
  - **Response:** None.

  **Server behavior:** This is triggered when someone uses SetChatSubject. The
  server sends the new subject to every client in the specified chat so they can
  update their display.

  **End-user experience:** As mentioned, the chat’s topic line refreshes to the
  new text. Many clients also print a line in the chat log like "**Topic**:
  `new subject`" to ensure everyone notices the change.

## Private Messaging (Instant Messages)

Aside from group chat, Hotline supports one-to-one direct messages often called
“private messages” or “instant messages.” These do not open a chat room;
instead, they are like sending a direct note to one user (which may open a small
private message window). The transactions for private messaging are separate
from chat transactions.

### Sending an Instant Message (Transaction 108) – Client Initiates

**ID 108 – Send Instant Message** (`myTran_SendInstantMsg`) is used when a user
sends a direct private message to another user on the server. **Purpose:** To
deliver a private message (which could be text, or a refusal/auto-response) from
one client to another via the server. **Initiator:** Client.

- **Parameters (Client→Server):** The sender specifies the target user’s ID
  (field 103). An **Options** field (113) is included to indicate the nature of
  this message. Options can be:

  - `1` for a normal user message,
  - `2` for a “Refuse message” (meaning this is actually a notice that the
    sender is refusing a prior message),
  - `3` for “Refuse chat” (similar concept for chat invites),
  - `4` for an “Automatic response” message.

  In a typical scenario, a user sending a new message will set Options = 1 (User
  message). If the user is replying with an auto-response or refusal, their
  client will use the appropriate code. The message text itself goes in field
  101 (Data) if there is a text message to send. Optionally, field 214 (Quoting
  message) can carry a quoted text (for example, if auto-responding to a
  specific message, it might quote it). In a normal initial message, quoting is
  not used.

- **Response:** There is no direct reply back to the sender for this
  transaction. The server will route the message to the intended recipient via a
  **Server Message (104)** transaction.

**Server behavior:** When the server receives SendInstantMsg, it checks that the
target user is online and that the sender has *Send Private Message* privilege
(privilege code 19). If allowed, the server will forward the message. It
translates the incoming data into a **Server Message** to the target user (see
below). The Options field tells the server if this is a normal message or some
kind of automated reply/refusal:

- If it’s a normal user message (1), the server simply delivers it.
- If the sender set it to “Refuse message” or “Refuse chat,” it likely means the
  sender is notifying the other person that their previous attempt was refused.
  In practice, the *receiving* user’s client, when set to refuse messages,
  doesn’t literally send a refusal code; instead the server or client on the
  other side might handle it. (Hotline protocol allows a user to actively send a
  refuse code – possibly used when someone tries to open a private chat and you
  hit “refuse” manually, your client might send an InstantMsg with option 2 or 3
  as a signal.)
- If it’s an “Automatic response” (4), the content will be the auto-reply text
  the user set. Typically this is triggered when a user is away; their client,
  upon receiving a message, might automatically send an InstantMsg with option 4
  back to the original sender (including the original message quoted in field
  214). The server then relays that auto-reply to the original sender as a
  Server Message (104).

If the target user is offline (which on a Hotline server shouldn’t happen – you
can only SendInstantMsg to connected users, since it’s not store-and-forward
email), the server might return an error or ignore it. If the target has “Refuse
Private Messages” enabled, one of two things can happen: either the server
blocks the message and sends back a refusal on their behalf, or the target’s
client immediately responds with a refusal message. In older versions, the
server might auto-reply using the user’s auto-response string if present. In
1.8.5, since the auto-response is known to the server (from transaction 304),
the server could potentially handle it. However, the design suggests the client
does the auto-reply by sending an InstantMsg with option 4.

**End-user experience:** When user A sends a private message to user B, user A
will see the message appear in a private chat-style window or message window
labeled with user B’s name (the Hotline client opens a small window for the
conversation). They won’t get an explicit “delivered” response, but if user B
replies, it will appear. If user B has auto-response on or is refusing messages,
user A might instantly receive a message back saying e.g. “User B is not
accepting private messages” or whatever auto-reply text B set. From user B’s
perspective, if not blocking, a window pops up with A’s message. If B has
“Refuse private messages” on, their client might not show anything, and either
nothing happens on A’s side (message ignored) or A receives a generic refusal
message. Typically, the Hotline client indicates with an icon that the user
cannot be messaged (so the user A would know not to even try, ideally). If A
does try, often nothing happens or A gets a system message from the server: the
**Server Message** could be used to inform them that user B isn’t accepting
messages.

### Receiving a Private Message (Transaction 104) – Server Initiates

**ID 104 – Server Message** (`myTran_ServerMsg`) is the mechanism by which a
user actually receives a private message or certain admin messages. Despite the
name “Server Message,” it generally carries messages from another user (routed
through the server) or from the server/admin to the user. **Purpose:** Deliver a
one-to-one message or notice to a client. **Initiator:** Server (whenever a
message or certain alerts need to be delivered).

- **Parameters (Server→Client):** In the common case of a user-to-user message,
  the server includes:

  - The sender’s user ID (field 103) and user name (field 102), so the recipient
    knows who it’s from.
  - An Options bitmap (field 113) indicating if this is an automatic response,
    or if the sender refuses chat/messages, etc., similar to before. For a
    normal incoming message, this is usually 0 or 1 (the bits might all be zero
    meaning a regular message). If it’s an auto-reply or a system/admin message,
    the flags differ.
  - The message text (field 101) which is the content to display.
  - If the message is quoting a previous one (like an auto-response quoting what
    you said), field 214 “Quoting message” contains the quoted text.

  If the server itself or an admin is sending a message, the protocol can omit
  the User ID field. The documentation says: if User ID (103) is not sent, the
  receiver should interpret the message as coming from the “server or admin”
  context, using the alternative fields. In that case, the server provides:

  - The message text (101) as usual.
  - A Chat options field (109) used here to signal message type: if Chat options
    = 1, it’s a “Server message”; if any other value, it’s an “Admin message”.
    This distinction might change how the client displays it (perhaps server
    messages are labeled as from “Server” or shown as system notifications).

- **Response:** Clients do not reply to ServerMsg transactions.

**Server behavior:** For a user-to-user PM, the server constructs a ServerMsg
containing the original sender’s info and text. It sets the Options field based
on the context (for example, if the sender had “automatic response” flag active
on their account, it might set the corresponding bit so the recipient’s client
can display an “Auto-Reply” tag). If the message is a **broadcast from an
admin** or a **server notice**, the server sends a ServerMsg without a user ID,
and uses the Chat options field to mark it. This is how things like an
administrator’s broadcast (which we’ll cover in Administration) or system
notifications (like “Server is restarting in 5 minutes”) are delivered to users
– they appear as a special message, often italicized or in a separate system
message window.

**End-user experience:** When a user receives a private message, a private
message window opens (or if one already exists for that correspondent, the new
message appears in it). It will show the sender’s name and the message text. If
it was an automatic response from the other side, it might say something like
“Auto-response from \[Name\]: I am away from my desk” – many clients visually
indicate auto-replies or refusals (maybe by prefixing “[Auto-Reply]”). If the
message was from the server or an admin (with no user ID), it might appear
either in a dedicated “News/Log window” or as a pop-up: for example, an admin
broadcast might show up as “**Admin Message**: Please note the server rules...”
to all users, or a server message might say “Welcome to the server!” when you
log in. In any case, the client treats these ServerMsg accordingly (knowing
whether there was a user ID or not).

**Note:** Hotline also had the concept of offline messages or news in older
versions, which used **Get Messages (101)** and **New Message (102)**
transactions. In 1.8.5, those are largely superseded by the News system (see
next section). For completeness: **Get Messages (101)** was a client request to
retrieve stored messages (server would reply with field 101 containing message
text for each message), and **New Message (102)** was a server push of a new
message (with field 101 text). These were used in older versions to implement
something like a bulletin board or message of the day. In modern use, they are
legacy and not commonly used since the News (bulletin board) transactions
replaced them. Implementers should be aware these exist but can be considered
deprecated in the context of 1.8.5.

## File Services (Files and Folders)

Hotline servers provide a shared file repository where users can browse folders,
download files, upload files, etc. The protocol uses a base port for
transactions and a separate data port for file transfers (by default, if the
server’s base port is N, file data connections use port N+1 for standard TCP
transfers). Below are the transactions for file system navigation and transfer:

### Listing Files in a Folder (Transaction 200) – Client Initiates

**ID 200 – Get File Name List** (`myTran_GetFileNameList`) is how a client asks
for the contents (files and subfolders) of a directory on the server.
**Purpose:** Retrieve a directory listing. **Initiator:** Client.

- **Parameters (Client→Server):** The client may send the path of the folder it
  wants to list (field 202 “File path”). If this field is not provided, the
  server assumes the root folder of the server’s file area. The path format is a
  binary structure in Hotline (not just a string – typically it’s a sequence of
  folder IDs or names). But the client takes care of that; the server just needs
  the provided path. No other fields are needed.
- **Response (Server→Client):** The server replies with one or more **File name
  with info** entries (field 200, repeated). Each entry contains a file or
  folder name along with metadata. According to the protocol, “File name with
  info” includes details like whether it’s a folder or file, size, possibly
  creation date, etc., plus the name. Essentially, the server is sending a
  directory listing of that folder. Each entry is optional in the sense the
  server can send as many as there are items.

**Server behavior:** Upon receiving the request, the server checks if the user
has permission to view that folder (there are folder-level privileges like “View
Drop Boxes” that might hide some folders) – in general, normal files are visible
if the user has *Download File* or *View* rights. Then it gathers all items in
the specified folder. For each file or subfolder, it creates a “file name with
info” record including the item’s name, size (if file), type/creator codes (for
Mac files), etc. If the listing is large, it might send multiple transactions
parts, but typically it can fit many entries in one response. The server sends
the list.

**End-user experience:** The user navigates in the client’s file browser (for
example, double-clicking a folder or choosing the Files view after login). The
client sends this request, and then the file list appears: you see filenames,
sizes, possibly icons indicating file vs folder. The user can now choose to
download files or enter subfolders (which would trigger more 200 requests for
deeper folders). If the folder was empty or not accessible, the user might see
nothing or an error (if no permission, the server might have sent an empty list
or possibly an error message).

### Downloading a File (Transaction 202) – Client Initiates

**ID 202 – Download File** (`myTran_DownloadFile`) is used when a client wants
to download a single file from the server. **Purpose:** Initiate a file
download. **Initiator:** Client.

- **Parameters (Client→Server):** The client provides the name of the file
  (field 201) and the path of the file (field 202) so the server knows which
  file to send. If the client is resuming a partial download, it can include a
  **File resume data** field (203) with resume information (Hotline supports
  resuming downloads; this data tells the server how much the client already
  has). There’s also an optional **File transfer options** (field 204) which in
  practice is used for specifying if the client wants a specific fork or format
  of the file. In Hotline 1.8.x, this is set to 2 for certain file types (TEXT,
  image files) to possibly indicate a preview or specific handling. For most
  downloads, the default value is 2.

- **Response (Server→Client):** The server responds with some fields indicating
  it’s ready to send. Specifically:

  - A **Transfer size** (field 108) which is the total size in bytes that will
    be sent (for this file).
  - The **File size** (field 207) of the file (could be same as transfer size,
    unless maybe a specific fork or conversion is happening; usually they
    match).
  - A **Reference number** (field 107) which is an identifier for this transfer.
  - A **Waiting count** (field 116) which tells how many downloads are in queue
    ahead of this one (if the server has limited slots). Usually if the server
    can start sending immediately, waiting count is 0. If all download slots are
    full, waiting count might be >0 and the server might not send data until a
    slot frees up.

**Server behavior:** When a download request comes in, the server first checks
the user’s privileges. The user must have *Download File* privilege for that
folder or file. If allowed, the server locks one of its download “slots” for
this transfer (Hotline servers often limit how many concurrent downloads they
serve per user or in total). If the slots are full, the server might queue the
request. In such a case, the server could respond with a **Download Info (211)**
transaction to indicate the user’s position in queue. (Transaction **211 –
Download Info** is a server-initiated message: it includes the reference number
and a waiting position count, basically telling the client “I’ve queued your
download, you are #N in line” – the client would typically display a “Waiting…”
status). In our scenario, assume either immediately or eventually a slot is
free:

- The server sends back the reply with reference number, file size, etc..
- Then the file transfer proper begins: The client is now expected to open a
  separate TCP connection to the server’s data port (base port + 1). The client
  sends a small handshake over that new connection containing `'HTXF'` (Hotline
  Transfer) and the reference number. The server uses that reference to match
  the transfer request.
- If the connection is established and handshake done, the server transmits the
  file’s data over that connection. Specifically, Hotline uses a “flattened file
  object” format which can include file forks and metadata for Mac files. For a
  simple file, it’s essentially the raw bytes preceded by a header.
- If the download was queued, once it’s this user’s turn the server will
  initiate the data connection handshake at that time (the client might keep
  trying to connect until allowed).
- If the client had provided resume info, the server will skip ahead and only
  send the remaining bytes (or instruct the client to resume from a point via a
  **Resume** action – though in Hotline’s case the resume was indicated by field
  203, so likely the server just starts from that offset).

The details of the actual data stream are complex, but the main point is the
transaction workflow sets it up. After sending data, the server closes the data
connection.

**End-user experience:** The user selects a file and clicks “Download”.
Immediately, an entry appears in their Transfers window. If the download starts
immediately, it shows progress (the client now downloading from the server). If
the server had too many concurrent downloads, the user might see a “Waiting in
queue (position X)” status – Hotline clients show the queue position, thanks to
the server’s Download Info (211) message which includes `Waiting count`. Once
it’s their turn, the transfer begins. If the transfer is interrupted, the user
can resume it later; the client will send the same 202 with a resume token and
the server will continue where left off. When complete, the user has the file
saved on their machine.

### Uploading a File (Transaction 203) – Client Initiates

**ID 203 – Upload File** (`myTran_UploadFile`) handles sending a file from the
client to the server. **Purpose:** Initiate an upload of a single file to the
specified server folder. **Initiator:** Client.

- **Parameters (Client→Server):** The client specifies the file name (field 201)
  and the destination path on the server (field 202). If the client is resuming
  an interrupted upload, it might include **File transfer options** (field 204)
  set to indicate resume (the spec notes it’s used to resume downloads, but in
  upload context it may be unused or set to a default). It also can provide the
  total file size upfront in field 108 (File transfer size) if not resuming.
  This helps the server know how much to expect.
- **Response (Server→Client):** The server replies with a **Reference number**
  (field 107) to identify this transfer, and optionally **File resume data**
  (field 203) if the upload is to be resumed. The resume data would be used if
  an upload was partial; it tells the client where to continue. Often for fresh
  uploads, resume data is not sent, meaning start from scratch.

**Server behavior:** When an upload request arrives, the server checks
privileges – the user must have *Upload File* rights for that folder. If allowed
and if there’s space/quotas okay, the server will allocate a transfer slot. The
reply gives a reference number like with downloads. Then the client is expected
to open a new connection to the server’s upload port (which is the same as
download port, base port+1, in non-HTTP mode). The client then sends the
`'HTXF'` handshake with the reference and the total data size to send. After
that, the client transmits the file data in the same “flattened file” format
over that connection. The server receives the bytes and writes the file to the
specified folder.

If the client had crashed and is resuming an upload, the server might have a
partial file and it can provide resume info: typically, Hotline supported
resuming of uploads, though it was less common. The `File resume data` field in
the reply, if present, would tell the client from which point to continue. The
client then would only send the missing portion, presumably after a negotiation
on the data connection (similar to how folder download resumes are done with a
small handshake, although specifics for upload resume aren’t detailed, likely
simpler: it sends reference and offset).

If upload slots are full, Hotline may queue the upload similar to downloads, but
the protocol does not specify a distinct “Upload Info” transaction. Possibly the
server could delay the reply until a slot frees or send an error indicating
busy. More likely, it just limits per-user or total and the client has to try
later.

**End-user experience:** The user selects a file to upload (via the client’s
interface, typically dragging a file into the server’s folder or using an
“Upload” command). The upload appears in their Transfers window. If it starts,
they see progress as data is sent. If the server was busy or they lacked
permission, the client would show an error (e.g., “Upload not allowed” or
“Server busy”). With appropriate permissions, the file will appear on the server
(other users might see it pop up in the file list as soon as it’s done, or if
the server lists incomplete uploads, maybe they see it appear mid-transfer but
usually not until complete). If connection breaks, the user can resume the
upload; the client will automatically try to resume when reinitiating, and the
server will append the rest.

### Deleting a File (Transaction 204) – Client Initiates

**ID 204 – Delete File** (`myTran_DeleteFile`) is used to remove a file from the
server’s filesystem. **Purpose:** Delete a specified file (or possibly an empty
folder) on the server. **Initiator:** Client.

- **Parameters:** The client provides the file name (field 201) and its path
  (field 202) to identify the item to delete.
- **Response:** None (if successful, the server just does it; if failure, the
  server might send an error or simply not remove it).

**Server behavior:** The server checks that the user has the right to delete
that item. There are separate privileges for deleting files and folders: *Delete
File* (privilege 0) for files and *Delete Folder* (privilege 6) for folders. If
the target is a file and the user has Delete File privilege in that folder (or
globally), the server deletes it from disk. If it’s a folder, the user would
need Delete Folder privilege and typically the folder must be empty (unless the
server also deletes contents recursively, which it likely does not for safety).
No reply is needed, but if an error occurs (like no permission or file not
found), the server could send an Error (100) with a message or use a Server
Message to inform the client.

**End-user experience:** The user triggers a delete (e.g., pressing delete key
on a highlighted file or using a context menu). If they have permission, the
file disappears from the file list on their client. Other users browsing that
folder might also see it disappear (the client would refresh the file list). If
the user lacked permission, they would get a notice “You are not allowed to
delete that file” (likely the server sends an error text, which the client shows
as a dialog or status message). If the file was successfully deleted, on most
clients there’s no specific success message – it just is gone.

### Creating a New Folder (Transaction 205) – Client Initiates

**ID 205 – New Folder** (`myTran_NewFolder`) lets a user create a new directory
on the server. **Purpose:** Create a new folder in the specified directory.
**Initiator:** Client.

- **Parameters:** The client supplies the desired name for the new folder (field
  201\) and the path of where to create it (field 202) (if path is not given,
  maybe create in root, but typically you specify the parent folder).
- **Response:** None (if creation succeeds, the server will likely send an
  update via the file list mechanism if needed).

**Server behavior:** The server checks the *Create Folder* privilege (privilege
5\) for that location. If allowed, it creates the directory on the server’s
filesystem. No direct reply is sent, but the client will usually follow up by
listing the folder’s contents (or the parent folder’s contents). Often the
client might automatically refresh the view by calling GetFileNameList again to
show the new folder.

**End-user experience:** The user hits “New Folder” in the client, enters a
folder name. The new folder then appears in the file list if creation was
successful. If they lacked rights, they’d see an error message. After creation,
they can navigate into that folder or upload files to it.

### Getting File/Folder Info (Transaction 206) – Client Initiates

**ID 206 – Get File Info** (`myTran_GetFileInfo`) retrieves metadata about a
file or folder. **Purpose:** To get detailed information such as file type,
dates, comments, etc., usually for a “Get Info” dialog. **Initiator:** Client.

- **Parameters:** The client specifies the target file/folder name (field 201)
  and its path (field 202). The path is optional if it’s in current directory.

- **Response:** The server replies with a set of fields describing the
  file/folder:

  - **Name** (field 201) – the file name again (possibly to confirm or in case
    of any encoding),
  - **File type string** (205) and **File creator string** (206) – these are
    4-character codes used on classic Mac OS to identify file type/creator. They
    might be blank for non-Mac files.
  - **File comment** (210) – a user-editable comment or description. Hotline
    allowed comments on files/folders (like a description visible to users).
  - **File type** (213) – likely an integer or flag indicating file vs folder
    (or type code numeric).
  - **Creation date** (208) and **Modification date** (209) – timestamps for the
    file.
  - **File size** (207) – size in bytes (for a folder, size might be 0 or
    cumulative size if server calculates it).

  This info covers all basic properties.

**Server behavior:** On GetFileInfo, the server reads the file’s metadata from
the filesystem. For files, on Mac it might store type/creator codes and comments
as extended attributes (Hotline servers on Windows might not have those, but
protocol still sends blanks). Comments are stored in the server’s data (Hotline
server maintains a database of comments for files/folders). The server checks if
the user has rights to see info (possibly *Get File Info* privilege, which is
likely general since all can usually get info – indeed there is privilege 24
“Get Client Info” but that’s for user info, not file; file info typically anyone
can view if they can see the file). It then populates the reply fields and sends
them.

**End-user experience:** The user selects a file and chooses “Get Info” (or
right-click Properties). A dialog appears showing details: file name, size, type
(perhaps showing the Mac type/creator or an icon for file type), creation and
modification dates, and the “Comment” field. The comment is often editable if
they have permission (in Hotline, file comments can be edited if you have
appropriate rights). So they might see a description or be able to add one.

### Setting File/Folder Info (Transaction 207) – Client Initiates

**ID 207 – Set File Info** (`myTran_SetFileInfo`) is used to change metadata of
a file or folder, such as renaming it or editing its comment. **Purpose:**
Modify file attributes on the server. **Initiator:** Client.

- **Parameters:** The client sends the target file’s current name (201) and path
  (202). It can include a new name (field 211 “File new name”) if renaming,
  and/or a new comment string (field 210) to set a comment/description. Either
  or both can be provided. (If the client only wants to edit the comment, it
  leaves new name blank; if only renaming, it leaves comment blank.)
- **Response:** None (the server performs the change and doesn’t explicitly
  confirm except by the effects).

**Server behavior:** The server checks privileges: to rename a file, the user
likely needs *Rename File* privilege (priv 3) or *Rename Folder* (7) if it’s a
folder; to set a comment, perhaps the same privilege or a specific one like *Set
File Comment* (28) / *Set Folder Comment* (29). Indeed, the protocol notes
access for SetFileInfo as requiring either Set File Comment or Set Folder
Comment privilege (priv 28 or 29). If the user only has comment privilege but
not rename, presumably they can change the comment but the server will ignore
any new name field. Conversely, if they have rename rights but not comment, they
can rename but not set comment. The server will apply the changes to the file
system: if renaming, it changes the file/folder name. If updating comment, it
stores the new comment in its database. The server does not send a direct reply,
but it might internally trigger an update: for example, if a folder name
changed, clients viewing the parent directory might need to refresh (the server
might rely on the client to refresh manually or could broadcast a file list
update via some mechanism, but Hotline protocol doesn’t have an explicit “file
list changed” push except maybe through the same channel as file events for
admin actions).

**End-user experience:** If the user has permission, after they rename a file or
edit its comment in the client’s Info dialog, the changes take effect. The file
will show the new name in the list. Others browsing that directory might or
might not see it update immediately — typically they would see it when they
refresh or when they next open the folder (Hotline doesn’t actively push file
list changes on renames, except the user who did it sees it immediately). If the
user lacks rights to rename or comment, their attempt will result in an error or
simply nothing happens. For instance, trying to rename a file without privilege
might cause the client to pop up “You do not have permission to rename files.”
If comment editing is not allowed, the comment field might be grayed out or the
server might reject the SetFileInfo silently.

### Moving Files/Folders (Transaction 208) – Client Initiates

**ID 208 – Move File** (`myTran_MoveFile`) is used to move or relocate a
file/folder from one directory to another on the server. **Purpose:** Cut-paste
a file or folder to a new location. **Initiator:** Client.

- **Parameters:** The client specifies the item’s name (201) and current path
  (202), and the destination path (field 212 “File new path”). Essentially,
  “move [this name] from [old path] to [new path]”.
- **Response:** None.

**Server behavior:** The server checks *Move File* privilege (priv 4) if it’s a
file, or *Move Folder* (8) for folders. If allowed, the server will remove the
item from the old directory and add it to the new directory (on the filesystem
this is a rename operation or file system move). This includes adjusting any
internal records (like comments might move with it; since it’s the same file
just in a new place, it’s straightforward). If the destination has a file with
the same name, the server might either overwrite (if allowed) or fail the move –
the protocol doesn’t specify, but typically it might fail to avoid overwriting
unless the user also has delete rights. The client is not explicitly told the
result, but if successful, the file will disappear from the old folder listing
and should appear in the new one.

**End-user experience:** The user can drag and drop a file from one folder to
another in the client interface. If they have permission, it will vanish from
the source folder view and show up in the target folder view (the client likely
issues a GetFileNameList on both source and destination to update them). If not
permitted, the client will pop up a message or just snap the file back,
indicating they can’t move it. Essentially it works like moving files in a file
explorer.

### Creating an Alias (Shortcut) (Transaction 209) – Client Initiates

**ID 209 – Make File Alias** (`myTran_MakeFileAlias`) creates an alias/shortcut
of a file on the server. **Purpose:** To create a reference to an existing file
in another folder (so one file can appear in two places). **Initiator:** Client.

- **Parameters:** The client specifies the source file name (201) and its
  current path (202), and a destination path where the alias should be created
  (212). The alias itself will typically have the same name or maybe a .alias
  extension depending on server, but the protocol doesn’t require a new name –
  it likely uses the same name in the new location.
- **Response:** None.

**Server behavior:** The user needs *Make Alias* privilege (priv 31) to do this.
The server will create an alias entry in the destination. On classic Mac HFS
servers, this could literally be a Finder alias file. In cross-platform context,
it might be a logical reference stored by the server. But from a Hotline
perspective, it shows up as a file that, when downloaded, actually gives the
original file’s content or points to it. The server likely treats an alias as a
special file type that when accessed, redirects to the original. No direct
feedback except the alias appears in listings.

**End-user experience:** The user might select “Make Alias” on a file, then
choose a destination folder. The alias appears as a new item in the target
folder, typically with an alias icon (a curved arrow or such). Other users see
it too. If a user tries to download the alias, the server will serve the
original file’s data (so it acts as a pointer). For the user who created it,
they effectively made a shortcut so the file is accessible from multiple
locations on the server (useful for organization). If permission lacked, they’d
get an error.

### Downloading an Entire Folder (Transaction 210) – Client Initiates

**ID 210 – Download Folder** (`myTran_DownloadFldr`) allows a user to download a
folder and all its contents in one go. **Purpose:** To retrieve a folder
recursively (the server will bundle files and subfolders). **Initiator:**
Client.

- **Parameters:** The client specifies the folder name (201) and its path (202)
  to identify the folder to download.

- **Response:** The server responds with:

  - **Folder item count** (220) – how many items (files) are going to be
    transferred in total,
  - **Reference number** (107) for this multi-file transfer,
  - **Transfer size** (108) which is the total size in bytes of all files in the
    folder (and subfolders),
  - **Waiting count** (116) if queued, similar concept to single file download.

**Server behavior:** Downloading a folder is more complex. The server again
checks *Download File* privilege (since it essentially is downloading multiple
files). If allowed, and if not too many concurrent transfers, it sets up a
reference number and calculates how many files and total bytes are involved in
that folder (this can take time if folder is large; it likely counts up front).
Then the client connects to the data port with the reference (just like a file
transfer). The protocol then goes into a *folder download loop*:

- The server will not just blast data, but rather coordinate file-by-file. As
  documented, once the data connection is open, the server sends a header
  indicating the next file’s path and type. The client then must respond with
  what action to take (next file, resume, or send file).
- Essentially, for each file in the folder, the server says “I have file X
  ready” and the client says “send it” (or “skip/resume this one” if it had some
  of it). This is done with small control messages over the data connection: the
  **Download folder action** codes: `3` = proceed to next file, `2` = resume
  from offset, `1` = send file now.
- The server then sends the file’s data (with size header). Then moves to next
  item.
- This continues for all files (and likely subfolders are traversed; the server
  likely organizes them into some flat sequence with path info).
- The folder’s directory structure is preserved by the path info each file’s
  header contains, so the client can reconstruct the hierarchy when saving.

If at any point all download slots were busy, the initial response would have
indicated a waiting count (like single file). The queue mechanism is similar.
Resuming a folder download is also possible: the client can say “resume” for a
particular file in the sequence if partially done.

**End-user experience:** The user requests to download a folder (some clients
allow dragging a folder to your computer or a “Download Folder” option). The
client likely asks where to save this folder, then initiates the transaction.
The experience is that multiple files start downloading as part of one job. The
client might show a single aggregate progress bar for the whole folder, or it
might show each file being downloaded sequentially. The user doesn’t have to
manually download each file – it’s automated. If the folder is large, it could
take a while; the user just sees progress. If interrupted, the client can later
resume the folder download, and it will skip files already done. This is very
convenient for batch transfers (like grabbing an entire directory of files at
once).

### Uploading an Entire Folder (Transaction 213) – Client Initiates

**ID 213 – Upload Folder** (`myTran_UploadFldr`) allows uploading a folder (with
subfolders) to the server in one operation. **Purpose:** Send multiple files
(and directory structure) to the server. **Initiator:** Client.

- **Parameters:** The client specifies the folder name it’s uploading as (201)
  and the destination path on server (202). It also sends the total size of all
  files in the folder (field 108) and the count of items (files) in the folder
  (220). `File transfer options` (204) may be present (set to 1 as per spec,
  possibly indicating “folder upload” mode).
- **Response:** The server returns a Reference number (107) for the transfer. No
  need to send resume data here (if resuming, likely handled in-band).

**Server behavior:** Privilege required is *Upload File* (1) since it’s
essentially many file uploads. After the handshake, the client connects to the
data port (as usual). The protocol for folder upload is essentially the inverse
of folder download: the client will send files one by one, with headers
indicating path, etc., and the server will respond with small control codes to
request next file, confirm receipt, or to handle resumes. The documentation
suggests the server can reply with a **Download folder action** code `3` (Next
file) to prompt the client to send the next item, or `2` to request resume data
if needed. The client then proceeds to send each file’s flattened data preceded
by a header (with path info). This continues until all files are sent.

Essentially, the server is reconstructing the folder on its side. It creates the
root folder (with the name given in 201) in the destination path, then as each
file entry comes in, it creates files and subdirectories accordingly.

If an error occurs mid-way (network drop), the server may have partial data;
resuming would involve the server telling the client which files were received
and which to continue. But implementing resume for multi-file upload is complex;
not sure if the official client supported it heavily. The protocol has
provisions though (resume codes).

**End-user experience:** The user uploads a folder (some clients allow dragging
a folder onto the server). The client will enumerate all files and maybe
compress them or just send structure. Typically, the user sees one composite
progress or a sequence of uploads happening automatically. On the server, the
folder with all its files appears. If they lack permission, none of it will
start (and they’ll get an error). If partially through the connection breaks,
some files might have uploaded; the user may need to re-upload the folder, which
might skip already uploaded files (depending on client sophistication). The
experience is akin to an FTP folder upload – the structure is preserved.

### Banner Download (Transaction 212) – Client Initiates

**ID 212 – Download Banner** (`myTran_DownloadBanner`) is a special-case
transaction to fetch the server’s banner image via the file transfer port.
**Purpose:** Retrieve the server’s banner graphic (if the server uses an image
banner instead of a URL). **Initiator:** Client.

- **Parameters:** None in the request. The client just says “give me the banner
  now.”
- **Response:** The server replies with a Reference number (107) and Transfer
  size (108) for the banner data.

**When/Why:** This occurs typically right after login. In the login sequence,
the server’s Show Agreement (109) message would have told the client if a banner
image is available by providing a Banner ID and possibly by setting banner type
to something like JPEG/GIF and not providing a URL. In that case, the client
knows it must perform DownloadBanner to actually get the image bytes to display.
Alternatively, if the banner type was URL, the client would just fetch from that
URL instead.

**Server behavior:** On DownloadBanner request, the server essentially treats
the banner file like a normal file transfer. It might have the banner image
stored on disk. It gives a reference and size, and then the client opens a data
connection (port+1) with the reference. The server sends the banner image data
over that connection (the Type field in the handshake is `2` meaning banner, as
the snippet suggests). The client receives it and displays it.

**End-user experience:** The user doesn’t explicitly do this; it’s automatic
after login. They might notice a small delay and then a banner image (maybe an
advertisement or server logo) appears in the client’s banner area. If the server
uses an external URL banner, the client loads it from the internet directly. If
an image was provided via this transaction, the effect is the same: a banner is
shown. If the user has banners disabled or none exists, nothing is
fetched/shown.

## News (Bulletin Board System)

Hotline servers include a “News” or bulletin board system where users can read
and post articles organized into bundles and categories (similar to forums or
newsgroups). In Hotline 1.8.5, the news system is hierarchical: **Bundles**
(top-level groupings, sometimes called “news folders”), containing
**Categories**, which can contain sub-categories or posts (articles). Each
article can have replies (forming threads). The protocol provides transactions
to navigate and manipulate this structure. (Note: older clients had a simpler
“messages” board, but 1.8.x uses the advanced news; we include the old **Old
Post News (103)** for legacy completeness.)

### News Hierarchy

Hotline implements its news as a four-tier hierarchy similar to a modern forum.
At the top are **bundles**, each identified by a path on the server. Inside a
bundle are **categories** that further divide the discussion. Users post
**articles** within a category, and each article may have **replies** forming a
threaded conversation. Articles expose metadata such as ID, title, poster, date
and data flavour (typically `"text/plain"` but extensible to formats like
`"text/markdown"`), plus the article body. Replies carry the same data fields
and additionally track the parent and first child article IDs to maintain the
thread structure.

### Listing News Categories (Transaction 370) – Client Initiates

**ID 370 – Get News Category Name List** (`myTran_GetNewsCatNameList`) is used
to retrieve the list of sub-categories (or top-level bundles) at a given news
path. **Purpose:** To list news groups or categories within a bundle.
**Initiator:** Client.

- **Parameters:** The client can specify a **News path** (field 325) indicating
  which part of the news hierarchy to list. If this is omitted, the server might
  return the top-level bundles. The news path is a structured reference (like
  “/” for root, or a bundle/category identifier).

- **Response:** The server replies with one or more **News category list data**
  entries (field 323, repeated). Each entry represents a bundle or category name
  (and possibly some encoded info like number of posts, etc., depending on
  implementation). Essentially, this is the list of category names at that
  level.

  *Compatibility:* The protocol notes that if the client/server version is older
  than 1.5, it would use field 320 instead of 323 for these entries (so field
  320 was the old identifier for category list data). But in 1.8.5 we use 323.

**Server behavior:** The server, upon request, looks at the specified path in
the news database:

- If the path is root or a bundle, it finds all immediate sub-categories (which
  could be bundles or actual categories containing articles, depending on level)
  and sends their names. For top-level, these entries are “Bundles” (top-level
  news groupings). Each entry likely includes the category’s name and maybe an
  ID or flag in the binary data, but from the client perspective just a name.
  The server sends them all.
- The server might require *News Read Article* privilege (priv 20) to access
  news at all, but typically reading news is allowed for all logged-in users by
  default.
- If no path given, return top-level bundles. If a path to a specific bundle,
  return its categories.

**End-user experience:** When the user goes to the News section in the client,
the client will first fetch the top-level list (by calling GetNewsCatNameList
with no path). The user then sees a list of news bundles (for example,
“Announcements”, “Discussion Board”, etc.). These appear kind of like folders.
If the user clicks a bundle to expand it, the client calls GetNewsCatNameList
with that bundle’s path, and then receives the categories inside (say, “Rules”,
“Q&A” categories, etc.), which then display. The user navigates these like
directories to find posts.

### Listing News Articles (Transaction 371) – Client Initiates

**ID 371 – Get News Article Name List** (`myTran_GetNewsArtNameList`) retrieves
the list of article titles under a given news category. **Purpose:** To list the
actual posts/articles (and possibly sub-threads) in a category. **Initiator:**
Client.

- **Parameters:** The client sends the News path (325) identifying which
  category (or sub-category) it wants the article list for.
- **Response:** The server replies with one or more **News article list data**
  entries (field 321, repeated). Each entry corresponds to an article (post)
  title in that category. It likely includes the article’s title and some
  identifier (like an ID and maybe author or date snippet). The client will
  typically display the list of post titles.

**Server behavior:** The server looks up all articles in the specified category
(if the category has sub-categories, the client would use 370 for those; 371 is
specifically used when reaching a level where actual articles exist). It then
sends each article’s info. This probably includes an article ID internally,
which the client will use to fetch the full content later (the protocol suggests
that field 321 contains necessary data, possibly including the article’s subject
and an ID). The server requires *News Read Article* privilege (priv 20) to read
posts, which if the user lacks, might return nothing or error.

**End-user experience:** When the user opens a news category (for example
“Announcements”), the client requests the list of articles there. The user then
sees a list of post titles, possibly with some indicator (unread/read). They can
select a post to read it. If the category had sub-categories instead of
articles, the client would have used 370 again rather than 371, so 371 results
indicate that this is actually a list of posts (the final level of browsing).

### Reading a News Article (Transaction 400) – Client Initiates

**ID 400 – Get News Article Data** (`myTran_GetNewsArtData`) is used to retrieve
the full content of a specific news article (post). **Purpose:** Download the
text (and metadata) of a news article so it can be read. **Initiator:** Client.

- **Parameters:** The client specifies the category path (field 325) where the
  article resides, the Article ID (field 326) of the desired post, and a **Data
  flavor** (field 327) indicating what format of the article it wants. In
  Hotline, the data flavor is typically `"text/plain"` (meaning we want the text
  content). The Article ID is an identifier the server assigns to each post
  (likely gotten from the list data).

- **Response:** The server replies with the article’s details:

  - **Title** (328) – the article’s title string.
  - **Poster** (329) – the username of who posted it.
  - **Date** (330) – the date/time it was posted.
  - **Previous article ID** (331) and **Next article ID** (332) – references to
    navigate threads (the previous and next article at the same level).
  - **Parent article ID** (335) and **First child article ID** (336) –
    references for threading (i.e., if this post is a reply, parent ID points to
    the post it replied to; first child ID points to the first reply to this
    post, if any).
  - **Data flavor** (327) echoing what is provided (should be `"text/plain"`).
  - **Article data** (333) – the actual content of the article, i.e., the body
    text. This field is optional in case the flavor is not text, but in our case
    it will contain the post’s text.

**Server behavior:** On request, the server loads the specified article from its
database. It ensures the user can read it (*News Read Article* priv required, as
above). It then sends all the metadata and the content. If the content is plain
text, it’s all in field 333. If there were other flavors (like attachments or
HTML), the protocol could handle but currently it’s plain text only and others
are ignored. The article IDs (previous/next/parent/child) allow the client to
implement “threaded” reading (like next/prev buttons or hierarchical view).

**End-user experience:** The user selects a post from the list. The content of
that post is then displayed in the client’s news reader pane: they see the
title, author, date, and the body text. The client might also provide buttons to
go to next or previous posts (which use the IDs provided) or to go “up” to the
list again. If the post is part of a threaded conversation, the client might
allow viewing replies (it could automatically fetch the first child, etc., or
just list replies as separate articles under the same category if the server
organizes them that way – implementations vary). The key is the user can now
read the full text of the article.

### Posting a News Article (Transaction 410) – Client Initiates

**ID 410 – Post News Article** (`myTran_PostNewsArt`) is used when a user
submits a new post (either a new thread or a reply) to the server’s news system.
**Purpose:** To create a new article in a given category (or as a reply to an
existing article). **Initiator:** Client.

- **Parameters:** The client sends:

  - The category path where to post (325).
  - An Article ID (326) that represents the parent article if this is a reply
    (the spec notes “ID of the parent article?”). If this is a new thread
    (posting at top of category), this might be 0 or omitted.
  - The article’s title (328).
  - Article flags (334) – possibly indicating if the post is locked, or
    announcement type, etc. Typically 0 for normal posts.
  - Data flavor (327) – usually `"text/plain"` indicating the content is plain
    text.
  - The article content (333) – the body text of the post.

- **Response:** None (the server either accepts and will notify others, or
  returns an error if something’s wrong).

**Server behavior:** The server checks *News Post Article* privilege (priv 21)
to see if the user can post in that category. If allowed, it creates the new
article entry in its database. It assigns a new Article ID to it. The parent ID
provided tells it if it’s a reply (if so, it will link the new post under the
parent’s thread). The server sets any flags (maybe not used much; possibly for
admin posts). Since there’s no direct reply, the client doesn’t automatically
get the new post’s ID from this transaction. Instead, the server will make it
visible to clients in that category on next refresh. In some implementations,
the server might immediately push a **New Message (102)** or some notification
to alert clients of the new post, but Hotline’s approach is often that clients
periodically refresh or the user will see it upon next check. The protocol does
define **New Message (102)** which could serve as “server pushes new news post”,
but it’s not clearly documented if 1.8.5 uses it for news. Many clients simply
refresh the list after posting.

**End-user experience:** The user writes a post via the client’s interface
(usually a text editor that pops up when you choose “Post News” or “Reply”).
Upon sending, if successful, their post appears in the category. Often the
client will refresh the article list, so the user sees their new post title in
the list of articles. Others currently viewing that category might have to
refresh manually or might also automatically see the new title pop in (some
clients auto-refresh every few seconds or when a new post transaction is
detected). If the user lacked permission to post (like a read-only forum), the
client would show an error message like “You do not have permission to post in
this category” (the server would have denied the transaction, possibly via an
error code). With success, the article is now stored, and other users can read
it.

### Deleting a News Article (Transaction 411) – Client Initiates

**ID 411 – Delete News Article** (`myTran_DelNewsArt`) is used to remove a
specific article (or thread) from the news board. **Purpose:** Delete a news
post. **Initiator:** Client.

- **Parameters:** The client provides the category path (325) and the Article ID
  (326) of the post to delete. There is also a flag (337, “News article –
  recursive delete”) indicating whether to delete child articles (replies) as
  well. This flag is 1 for deleting the post and all its replies (the entire
  thread), or 0 to delete just the single article (if 0, and there are replies,
  those might become orphan or perhaps the server disallows non-recursive
  deletion if replies exist).
- **Response:** None (server performs deletion).

**Server behavior:** The server checks *News Delete Article* privilege (priv
33). Typically only admins or moderators have this. If allowed, and if the
article exists, the server will remove it. If recursive flag is 1, it deletes
all replies recursively. If 0 and replies exist, the behavior might be to only
delete that post’s content (maybe leaving a placeholder saying removed), or it
might refuse unless no replies. The protocol allows specifying it, so presumably
if 0 and thread has children, it might just delete that one and leave children
as orphan (maybe shifting them up under the parent’s parent). The server likely
also updates indexes, etc.

There’s no specific “notification” to other users that a post was deleted except
that it will disappear from the list. The server might not push that info;
clients might find out on next refresh or if they attempt to read it and get an
error. Possibly an admin delete could be accompanied by a broadcast or not.

**End-user experience:** Only authorized users (admin or moderator) can delete
posts. If a user deletes their own post (if the server allowed that – often not
unless they have privilege or maybe the server allows authors to remove their
own within some timeframe), the post will vanish from the category listing. If
others have the list open, they might not see it gone until refresh. If someone
tries to read a deleted post, they might get “Article not found” error if not
refreshed. So essentially, the post and its replies (if chosen) are gone as if
they never were there. Users might just notice that something disappeared. There
is no “undo” – it’s a permanent removal from the server’s perspective.

### Managing News Structure (Transactions 380, 381, 382) – Client Initiates (Admin/Mods)

These transactions let privileged users manage the news hierarchy (creating or
deleting bundles/categories):

- **ID 381 – New News Folder** (`myTran_NewNewsFldr`) is for creating a new
  **News Bundle** (top-level folder). **Purpose:** Create a new bundle in the
  news system. **Initiator:** Client (admin). Requires privilege *News Create
  Folder* (priv 36).

  - **Parameters:** The client sends a name for the new bundle (201) and the
    news path (325) under which to create it. For a top-level bundle, the path
    might be empty or some root reference.
  - **Response:** None.

  **Effect:** The server creates a new bundle (a container that can hold
  categories). In practice, top-level news bundles might just be created at root
  (so path blank, name provided). If path is given (like specifying an existing
  bundle), it might create a sub-bundle, but typically there are only two
  levels: bundles and categories. The user who created it (admin) and others
  will see this new bundle appear in the news list (probably next time they
  refresh or maybe immediately if the client refreshes on creation).

  **End-user experience:** The admin uses an option “Create Bundle” in their
  client (which might be enabled for admins). They name the new bundle. Users
  will see a new top-level entry in the news section by that name, which can
  then be expanded (initially empty). If normal users are not allowed to create
  bundles, they won’t have that option.

- **ID 382 – New News Category** (`myTran_NewNewsCat`) creates a new
  **Category** under an existing bundle. **Purpose:** Add a sub-category (forum)
  under a news bundle. **Initiator:** Client (admin or privileged user).
  Requires *News Create Category* privilege (priv 34).

  - **Parameters:** The client provides the category name (field 322) and the
    news path (325) where to create it. The path would be the bundle (or parent
    category) under which to make this new category.
  - **Response:** None.

  **Effect:** The server creates a new category node. Users will see it when
  viewing that bundle.

  **End-user experience:** The admin (or user with rights) chooses “Create
  Category” (if allowed) and names it. It appears as a folder under the selected
  bundle in the news list. For example, under “Announcements” bundle, an admin
  might add a category “Server Updates”. Regular users typically cannot create
  new categories unless the server admin specifically grants certain accounts
  that privilege.

- **ID 380 – Delete News Item** (`myTran_DelNewsItem`) deletes a **Bundle or
  Category** (an organizational item). **Purpose:** Remove a news folder (either
  a top-level bundle or a sub-category). **Initiator:** Client (admin). Requires
  either *News Delete Folder* (priv 37) or *News Delete Category* (35) depending
  on target.

  - **Parameters:** The client sends the path (325) of the news item to delete.
    This path would point to a bundle or category. There is no explicit field to
    distinguish bundle vs category; the server can infer from internal data or
    privileges used.
  - **Response:** None.

  **Effect:** The server will remove that entire bundle or category and all
  articles within it. This is a destructive operation. If it’s a bundle, all
  categories and posts under it are gone. If it’s a sub-category, its posts are
  gone. The server would likely not allow deletion of a bundle if the user only
  has category deletion privilege or vice versa. It might require the
  appropriate one or both privileges.

  **End-user experience:** The admin might click “Delete Category” or “Delete
  Bundle” on an item in the news list (these options probably only appear for
  those with rights). Once confirmed, that item disappears from everyone’s view.
  All posts in it become inaccessible. Essentially, an entire section of the
  board is removed. Normal users wouldn’t have this ability, so they would just
  notice that a forum is gone if an admin removed it (maybe accompanied by an
  announcement or not).

**Legacy Note:** **ID 103 – Old Post News** (`myTran_OldPostNews`) is a legacy
transaction from older Hotline versions. It allowed posting a news message in
the old single “News” window system (pre-1.5). It takes just a Data field (101)
for the message text, with no category since older servers didn’t have multiple
categories, just one stream of messages. It required *News Post Article*
privilege (21). In 1.8.5, this is not used for the new system but might still be
supported for backward compatibility with very old clients. Essentially, if an
old client sends OldPostNews, the server might stuff the message into a default
category or ignore it. Modern implementations can mostly ignore 103, focusing on
the new news transactions above.

## Administrative Functions

Finally, Hotline protocol includes transactions reserved for administrative
control of the server and user accounts. These are typically only usable by the
admin or users with specific privileges. They allow remote management of user
accounts (adding or removing allowed users), broadcasting messages to all users,
and forcibly disconnecting users.

### User Account Management – Client Initiates (Admin) (Tx 350–353)

These let an admin manage the server’s user database (the list of login accounts
allowed on the server). They correspond to features in the Hotline Admin client
interface for adding/editing accounts.

- **ID 350 – New User** (`myTran_NewUser`) allows the admin to create a new user
  account on the server. **Purpose:** Add a login account to the server’s list
  of allowed users. **Initiator:** Client (Admin).

  - **Parameters:** The admin supplies the new account’s login name (105),
    password (106), full user name (102), and an **access privileges bitmap**
    (110). The login is the username used to log in; password will be stored
    (Hotline may store it plaintext or hashed – in protocol it’s likely
    plaintext or simply negated bytes in older style, but as far as the
    transaction, it’s provided). The user name (102) is the display name (often
    initially set the same as login or a more descriptive name). The access
    bitmap is crucial: it defines what privileges the account will have (e.g.
    admin, file access, etc.). Privileges are assigned by setting bits according
    to [Access Privilege Bits](#access-privilege-bits-field-110). In the admin
    UI, this corresponds to checking boxes for privileges, which then form this
    bitmap.
  - **Response:** None direct.

  **Server behavior:** The server creates a new account entry in its user
  database (or memory if runtime). It will store the login, (likely hash the
  password internally or store it in config), the display name, and the
  privileges. If an account with that login already exists, the server would
  likely either overwrite or (more likely) not create and possibly return an
  error (maybe via an error transaction). But typically the admin client would
  prevent duplicates by checking the list first. No specific success message is
  sent; the admin client knows it succeeded if no error and the account appears
  in the list when fetched again.

  **End-user experience:** Only admins do this. In the admin’s Hotline client or
  Admin tool, when they add a user account and set privileges, behind the scenes
  it sends NewUser. The admin sees the new account appear in the server’s
  account list UI. Regular users don’t see anything – this is purely
  administrative (it doesn’t broadcast to all users that an account was added or
  anything).

- **ID 351 – Delete User** (`myTran_DeleteUser`) removes an existing user
  account from the server’s allowed list. **Purpose:** Delete a user’s account
  (they will no longer be able to log in). **Initiator:** Client (Admin).

  - **Parameters:** The admin provides the login name (105) of the account to
    delete. That identifies which account to remove.
  - **Response:** None (if success, the account is gone; if the account didn’t
    exist, server may ignore or error).

  **Server behavior:** The server will remove that user from its stored list. If
  that user is currently online at the time, an admin would typically kick them
  separately if desired – deleting an account doesn’t automatically disconnect
  them (but it does prevent re-login). There’s no broadcast or notification to
  regular users. If the admin accidentally tries to delete a non-existent user,
  likely nothing happens or an error is returned. The admin’s account list will
  update (the entry disappears).

  **End-user experience:** For the admin, the account disappears from the list
  in the admin tool. For the user whose account was deleted: if they are online,
  nothing immediate might happen (they could continue their session until
  disconnected manually or by leaving). However, once they log out, they can’t
  log back in. If an admin deletes an account and wants to kick the user off
  now, they would use Disconnect User (110) as well. There is one special case:
  the server might protect deletion of the built-in Admin account via privileges
  (so you can’t delete the last admin or yourself while logged in – just
  caution, but at protocol level, nothing stops sending 351 for any login).

- **ID 352 – Get User** (`myTran_GetUser`) retrieves information about an
  existing user account. **Purpose:** To fetch the details of a specific user
  account (used when editing an account). **Initiator:** Client (Admin).

  - **Parameters:** The admin sends the login name (105) of the account they
    want to view.
  - **Response:** The server replies with that account’s info: it returns the
    user’s full display name (102), the login (105) (interestingly, the protocol
    notes each character is bitwise negated in this field in the reply – an old
    obscure practice possibly for not sending plain text; the admin client will
    invert the bits again to get actual login), the password (106) (likely also
    negated or hashed similarly), and the access privileges bitmap (110). Refer
    to [Access Privilege Bits](#access-privilege-bits-field-110) to decode the
    bitmap.

  **Server behavior:** When an admin wants to edit an account, their client
  issues GetUser. The server looks up that account in its list and sends back
  the current stored values for name, login, (maybe an encoded password), and
  privileges. The weird negation (~) of each character for login (and possibly
  password) is probably done so that if someone is sniffing the connection, they
  don’t see the literal credentials easily. (It’s not true encryption but a
  simple obfuscation – historically Hotline did that). The admin client will
  invert those bits to display the actual login and password in the UI.

  **End-user experience:** Only the admin doing this sees anything – it
  populates the Edit User dialog with the user’s info. Regular connected users
  are not affected or informed.

- **ID 353 – Set User** (`myTran_SetUser`) updates an existing user account’s
  information. **Purpose:** Save changes made to a user’s account (login,
  password, name, privileges). **Initiator:** Client (Admin).

  - **Parameters:** The admin sends the account’s login (105) and new password
    (106) (if they changed it; if not changed, it might send the old one or some
    indicator), the new full name (102), and new access privileges bitmap (110).
    These privilege bits are the same as those in
    [Access Privilege Bits](#access-privilege-bits-field-110). Essentially the
    same fields as NewUser, but targeted at an existing user identified by the
    login.
  - **Response:** None.

  **Server behavior:** The server finds that account and updates the provided
  fields. If the login itself was changed via this (e.g., renaming the account),
  it will update the account’s key (though some systems might treat login as
  immutable; Hotline allowed editing login names IIRC). The server then uses the
  new values henceforth. If the user corresponding to that account is currently
  online, the server might also update some of their session info: for example,
  if their privileges were changed, the server may immediately enforce that. In
  fact, when an admin edits privileges of a currently online user, the server
  will send that user a new **User Access (354)** transaction to update their
  permissions live. The protocol 354 is “Set access privileges for current user”
  initiated by server – likely the server uses it in this scenario. So if the
  admin removed someone’s ability to download files on the fly, the user might
  get a UserAccess update dropping that bit, and their client would grey out the
  download button immediately. (The documentation doesn’t explicitly tie 354 to
  SetUser, but logically that’s how a live change would be communicated.)

  Password changes take effect (the user will need to use the new password on
  next login; if they’re online, it doesn’t boot them or inform them).

  **End-user experience:** The admin uses an “Edit Account” dialog, changes some
  settings, and saves. They see the account list updated (maybe icon color
  changes if privileges changed, etc.). If the edited user is online and their
  privileges were altered, they might notice something: their client could
  immediately reflect new privileges (for instance, if now they are an admin,
  they might see new admin options appear, or if some privilege revoked, an
  option disappears). In practice, Hotline might not always live-update
  privileges, but the protocol support is there. At minimum, the effect will be
  in their next session. They are not explicitly notified “Your account was
  edited” (unless the admin tells them). If their password was changed while
  they’re on, they won’t know until they try to relogin later.

### Disconnecting a User (Transaction 110 & 111) – Admin/Server Use

**ID 110 – Disconnect User** (`myTran_DisconnectUser`) is used by an admin (or
potentially the server itself) to forcibly disconnect a user from the server.
**Purpose:** Kick a user off (with optional banning). **Initiator:** Client
(Admin).

- **Parameters:** The admin specifies the target’s User ID (103). There’s an
  **Options** field (113) which can carry “ban options” – e.g., whether this
  should also ban the user’s account or IP. The exact encoding of ban options
  isn’t detailed in the doc, but likely a bit like 1 = ban IP, 2 = ban user,
  etc., if used. There is also an optional Data (101) which might be labeled
  “Name?” in the spec – possibly to provide a reason or the name of who’s
  banning? The documentation is a bit unclear on that field’s purpose.
- **Response:** None (the action is taken, then the server will inform the user
  being kicked via transaction 111).

**Server behavior:** When an admin issues DisconnectUser, the server immediately
disconnects that user’s session. It will typically mark them as offline and send
out a Notify Delete User (302) to others. If ban options were set, the server
also adds the user to a ban list (e.g., by IP or account name). Hotline servers
have a banlist feature (one can ban by address or account for a duration). The
server does not reply to the admin client, but the admin client will reflect
that the user is gone from user list. If the user cannot be disconnected
(perhaps if they have “Cannot be disconnected” privilege, priv 23, which could
be set for the Admin account to prevent other admins kicking each other), then
the server might do nothing or send an error back (or just ignore the request
for that ID). Privilege required to use DisconnectUser is *Disconnect User* priv
(22), which typically only Admin accounts have.

**End-user experience (target user):** The user being kicked will suddenly get
disconnected from the server. Their client will usually show a message like “You
have been disconnected by an administrator.” This message is provided by
**Disconnect Message (111)** which the server sends just before dropping the
connection. We cover that next. If banned, they will also find they cannot
reconnect (the server will refuse future login attempts, either indefinitely or
for a set time if the server uses temporary bans). This is effectively a “kick”
(and optional ban).

**ID 111 – Disconnect Message** (`myTran_DisconnectMsg`) is the server telling a
user that they are about to be (or have been) disconnected. **Purpose:** To
convey a reason or message on disconnect, then instruct the client to close.
**Initiator:** Server.

- **Parameters:** A Data field (101) containing the message to display to the
  user upon disconnect. This is mandatory when used.
- **Response:** None (the client is expected to close the connection after
  receiving it).

**Server behavior:** When the server is about to disconnect a user (either via
admin action, or maybe due to inactivity timeout or other reasons), it sends
DisconnectMsg with a reason text (for example, “You have been kicked by Admin”
or “Server shutting down”). Immediately after sending this, the server closes
the connection from its side as well. The user ID of the target is implicit
(since it’s sent on that user’s connection).

**End-user experience:** The user sees a message popup or in the status:
whatever text the server sent. Commonly if an admin kicked them, they might see
“Disconnected by Server: `<message>`”. Then the connection is lost – the client
returns to the server list or login screen. This is more graceful than a silent
drop; the user at least knows the cause. If the server simply dropped without
sending 111, the user would just see “Connection lost.”

### Broadcasting a Message (Transaction 355) – Admin/Server Initiated

**ID 355 – User Broadcast** (`myTran_UserBroadcast`) allows an admin or
privileged user to send a message to all online users. **Purpose:** To broadcast
an announcement. **Initiator:** Client (Admin) *and* also can be Server.

- **Parameters (Client→Server):** The admin client sends the text of the message
  in field 101 (Data). No other fields are needed.

- **Response:** None (server will forward it).

- **Server as initiator:** The server itself (like via a scheduled message or
  console command) can also initiate a broadcast. In that case, it sends
  UserBroadcast with field 101 containing the message and possibly treats it as
  an “Administrator message” on clients. The protocol notes when server
  initiates, it uses the Data field for the “Administrator message” text.

**Server behavior:** Upon receiving a UserBroadcast from an admin, the server
will take that text and send it out to every connected user as a **Server
Message (104)** with no user ID (or it might use the dedicated broadcast
mechanism). The spec actually defines UserBroadcast as a transaction that can be
initiated by both client and server. Possibly, the server could use transaction
355 to deliver the broadcast to each client too, but it’s more likely it just
uses the existing ServerMsg for actual delivery. However, since 355 exists,
maybe the server simply relays the same 355 to all clients (with Initiator:
Server for them). The documentation indicates: “The server can also be an
initiator of this transaction”. In that scenario, presumably clients receiving a
UserBroadcast treat it similarly to receiving a ServerMsg. Either way, the
message gets to everyone.

Privilege required to send a broadcast via 355 is *Broadcast* priv (32), which
typically only admin or moderators have.

**End-user experience:** All users will see a message appear, usually in a
separate system message window or as a highlighted text, often prefixed by
something like “Broadcast from Admin: `<text>`”. For example, an admin might
announce “Server will restart in 5 minutes.” Users see that in real-time. It’s
not part of public chat; it’s a distinct message, often modal or clearly
different (Hotline client had a floating window or a different color for admin
broadcasts). They cannot reply to it (except via PM or such privately). It’s
essentially like a server announcement PA system.

### Additional Admin Privileges Note

The admin can also use some of the normal user transactions in special ways:

- They can invite themselves into any private chat, or disconnect a chat (there
  was mention of Open Chat/Close Chat privileges (11,12) in the list, which
  possibly allowed forcibly entering or closing chat rooms).
- “Cannot be disconnected” privilege (23) if set on an account means no one can
  kick that account via 110 – the server would refuse a disconnect attempt,
  ensuring the main admin can’t be booted by a subordinate.

Administration tasks like setting the server message of the day or agreements
are done out-of-band (like editing a text file or using server config UI, not
via a specific transaction except in global server context, which we skip).

**Summary of Admin functions:** Account management (350-353) lets you maintain
who can log in and what they can do. The disconnect and broadcast tools (110,
111, 355) let you moderate users in real-time and communicate announcements.
These transactions are protected by privileges to prevent abuse. Implementers
should ensure that only authorized sessions (e.g., logged in as Admin or with
appropriate priv bits in User Access) can invoke them, and that they correctly
propagate their effects (e.g., update privileges immediately, send
notifications, etc.). Regular users never directly use these transactions.

______________________________________________________________________

**References:**

- Hotline Protocol Specification (v1.8.x) – for transaction definitions and
  fields.
- Hotline Server Configuration Manual – for context on admin actions and user
  experience (account icons and privileges).

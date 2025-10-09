# Migration Plan: Moving `mxd` Protocol Implementation to `wireframe`

This plan outlines the migration of the `mxd` server’s Hotline protocol
handling to the `wireframe` framework, replacing the custom networking code
with `wireframe`’s abstractions[^1]. The goal is to carry over **all existing
protocol functionality** from `mxd` (handshake, transaction framing, command
handling, etc.) into `wireframe` with fidelity. Compatibility with the Synapse
Hotline X client (SynHX, also known as *shx*) and the classic Hotline 1.9
protocol is a priority for baseline support. Since the project is pre-release,
it is acceptable to introduce breaking changes in internals to achieve a clean
migration. The primary deployment targets are Linux (x86_64 and aarch64), with
future portability to FreeBSD and macOS considered. The following steps detail
the migration process for an implementing software engineer.

## 1. Project Initialization

Begin by setting up a new server binary that uses `wireframe` while preserving
`mxd`’s business logic as a library. This creates a fresh entry point for the
application using the new framework, without disrupting the core features
already implemented.

- **Create a New Binary Crate**: Establish a new binary (e.g.
  `mxd-wireframe-server`) within the project. This will contain the
  `wireframe`-based server application. Configure its `Cargo.toml` to depend on
  the existing `mxd` library (where domain logic resides) and on the
  `wireframe` crate.

- **Refactor Core Logic into Library (if needed)**: Ensure that protocol logic
  in `mxd` (e.g. command parsing, database interactions) is accessible as a
  library. If `mxd` currently has a lot of code in `main.rs`, move the relevant
  parts into reusable modules. The new `wireframe` server will call into these
  functions for handling requests.

- **Add `wireframe` Dependency**: Include **wireframe** in the Cargo
  dependencies. This provides the building blocks for asynchronous binary
  protocols (preambles, custom framing, routing, etc.)[^2]. Verify that
  `wireframe` and its prerequisites (like Tokio) compile on both x86_64 and
  aarch64 Linux.

- **Deprecate the Old Frame Handler**: Plan to remove or disable the bespoke
  networking loop in `mxd` once the new server is operational. The `wireframe`
  library will take over responsibilities for connection handling and frame
  parsing, simplifying the codebase by avoiding parallel implementations.
  (Since the project isn’t released yet, this removal will not affect any
  external users.)

## 2. Implement the Handshake

The Hotline protocol’s session-initialisation handshake is the first message
exchanged. Under `wireframe`, this is handled via the **preamble** mechanism,
which allows custom handshake bytes to be read before normal message framing
begins.

- **Define a Handshake Preamble**: Create a struct representing the 12-byte
  client handshake message (4-byte protocol ID, 4-byte sub-protocol ID, 2-byte
  version, 2-byte sub-version[^3]). Implement `bincode::Decode`/`BorrowDecode`
  for this struct so that it meets `wireframe::preamble::Preamble` trait
  bounds[^4]. This lets `wireframe` know how to decode the incoming handshake
  bytes into a structured form.

- **Register Preamble with Server**: Configure the `WireframeServer` to use the
  custom handshake type by calling `.with_preamble::<YourHandshakeType>()`.
  Then use `.on_preamble_decode_success(...)` to attach a handler that runs
  when a new connection’s handshake is successfully decoded[^5]. In this
  handler, implement the handshake logic:

- **Validate Protocol ID and Version**: Check that the client’s protocol ID is
  `"TRTP"` and the version is supported (e.g. 0x0001)[^6]. If not, prepare to
  send an error.

- **Send Handshake Reply**: Use the existing `mxd` logic to send back the
  8-byte handshake reply (protocol ID + 32-bit error code)[^7]. On success, the
  error code is 0; on failure (bad protocol or unsupported version), send the
  corresponding error code (e.g. 1 or 2) and terminate the connection. This
  mirrors the current `parse_handshake` and `write_handshake_reply` behaviour
  in `mxd`.

- **Store Handshake Info**: Record the handshake details (such as
  `sub_version`) in the connection state if needed. For example, the server
  might retain the client’s sub-version number to adjust later protocol
  handling for compatibility.

- **Timeout and Error Handling**: Leverage `wireframe` or Tokio to enforce a
  handshake timeout (e.g. 5 seconds, as in `mxd`’s `HANDSHAKE_TIMEOUT`[^8]). If
  the handshake bytes don’t arrive or decode in time, drop the connection. Use
  `.on_preamble_decode_failure` to log or handle decode errors (like receiving
  malformed handshake bytes) – for instance, respond with no reply or a failure
  reply, then close.

## 3. Transaction Framing

After a successful handshake, all client-server communication uses the Hotline
**transaction frame** format. Each request or response is preceded by a fixed
20-byte header, followed by an optional payload. The new server must interpret
this framing using `wireframe`’s customizable serialization layer.

- **Implement a Custom Serializer**: Provide a `wireframe::Serializer`
  implementation that knows how to parse and format Hotline frames. This
  serializer’s `deserialize` method should:

- Read and parse the 20-byte header of an incoming message. The header layout
  includes fields for flags, request/reply marker, transaction type,
  transaction ID, error code, total payload size, and fragment data size[^9].
  You can reuse `mxd`’s `FrameHeader` struct and its parsing logic
  here[^10][^11].

- Determine if the message is fragmented. In Hotline, if
  `Data size < Total size`, the message is split across multiple TCP
  frames[^12]. Accumulate the bytes from subsequent frames (which share the
  same transaction ID and header values) until the full `Total size` of payload
  is reached[^13]. Only then should the combined payload be considered a
  complete message to dispatch. This reassembly can be done within the
  serializer or via a higher-level buffer in the connection state.

- Return a decoded *Envelope* or message struct that contains the transaction
  type, an optional correlation (transaction ID), and the payload bytes.
  `wireframe` will then route this message to the appropriate handler.

- **Handle Outgoing Frames**: Ensure the serializer’s `serialize` method
  performs the inverse operation for server replies. It should take a response
  message (with a specified transaction type/ID, payload, and error code if
  any) and produce the header + payload bytes ready to send. You can utilize
  `mxd`’s existing frame construction utilities (e.g. functions to write 16-bit
  and 32-bit big-endian values[^14]). The `FrameHeader.write_bytes` method in
  `mxd` can help format the header correctly[^15].

- **Validate Frame Contents**: Preserve the validation checks present in `mxd`.
  For instance, enforce the maximum payload size (e.g. 1 MiB)[^16] and ensure
  that the length fields make sense (the total size field should match the
  actual accumulated payload length, etc.)[^17]. If a frame is malformed or
  exceeds limits, the serializer should return an error so the connection can
  be dropped or an error response sent. This ensures robustness against corrupt
  or malicious inputs.

## 4. Command Dispatch and Routing

With the framing in place, the next step is to dispatch incoming transactions
to the appropriate logic. `wireframe` provides a routing mechanism similar to a
web framework, allowing the server to call different handler functions based on
the message type.

- **Define Message/Transaction Types**: Represent each Hotline transaction ID
  in a form that can be used with `wireframe`’s router. For example, if `mxd`
  has an enum `TransactionType` for the numeric IDs[^18], reuse or adapt it.
  These types (e.g. Login = 0x006B, GetFileList = 0x006C, etc.) will serve as
  the keys for routing.

- **Register Routes in WireframeApp**: In the `WireframeApp` builder, use
  `.route(<Type>, handler_fn)` to map each transaction type to its handler. For
  instance, `.route(MessageType::Login, handle_login)` would ensure that a
  `Login` request calls the `handle_login` function[^19]. Do this for all
  implemented commands (login, file queries, news posts, etc.).

- **Implement Handler Functions**: For each route, write an async function that
  takes the parsed request (e.g. `Message<LoginRequest>`) and returns a
  response (implementing `Responder`). Inside, bridge to the existing `mxd`
  logic:

- Parse the request’s payload into high-level parameters. You can leverage
  `mxd`’s parameter decoding helpers like `decode_params_map` and
  field-specific extractors (e.g. `first_param_string`) to interpret the
  payload[^20][^21].

- Invoke the same processing routines `mxd` used. For example, use or port the
  `handle_login` function (to verify credentials and update session) or call
  the database query functions for file listings (`list_files_for_user`, etc.).
  The goal is to avoid rewriting the business logic – instead, wrap it in the
  new handler interfaces.

- Construct a reply `Transaction` with the appropriate reply ID and any output
  parameters. You can use `mxd`’s `reply_header` helper to create a response
  header echoing the request ID[^22], and `encode_params` to build the payload
  bytes if needed. Then return this from the handler; `wireframe` will
  serialize and send it.

- **Manage Session State**: Preserve per-connection state across transactions.
  In `mxd`, a `Session` struct tracked data like the logged-in user ID for each
  connection[^23]. Under `wireframe`, utilise its session or context features
  to store this. For instance, implement the `WireframeProtocol` trait’s
  connection initialization to attach a new `Session` object to each connection
  (perhaps via `SessionRegistry` or by storing it in a thread-local context
  accessible to handlers). Then, in each handler, access the session to check
  authentication, update user state, etc. This ensures that, for example, after
  a successful Login transaction, the user’s ID is stored and later commands
  (file listings, news posting) can verify permissions via the session.

## 5. SynHX and Hotline 1.9 Compatibility

To support the SynHX client and Hotline 1.9 protocol quirks, the new
implementation must accommodate slight differences in how data is encoded or
interpreted. The following considerations ensure backward compatibility with
these clients:

- **Flexible Integer Encoding**: Hotline’s parameter format allows integers to
  be sent as 2-byte or 4-byte values depending on size[^24]. Make sure the
  parameter decoder in the new server handles both. For example, a field marked
  as an integer may come in as two bytes (if the value is small) or four bytes
  (if larger). The parsing logic should read the field length to decide whether
  to use 16-bit or 32-bit decoding. This was a known necessity for older
  clients and is already reflected in the protocol spec.

- **Encoded Strings (XOR Obfuscation)**: Some Hotline implementations
  (including SynHX) “encode” certain strings by XOR-ing each byte with `0xFF`
  before sending, as a simple obfuscation. The server must detect and decode
  these. Specifically, for fields that are expected to be text (such as
  passwords or maybe user messages) and where the client’s sub-version or other
  indicator suggests an XOR encoding, the server should invert the bytes (XOR
  again with 0xFF) to retrieve the actual string. This step should be done
  early in request handling so that all business logic operates on real
  plaintext values. Conversely, when sending responses, if communicating with a
  client that expects XOR-encoded strings, perform the XOR before framing the
  data.

- **Version-Specific Behaviour**: Use the handshake’s `sub_version` field to
  toggle compatibility behaviors[^25]. For instance, if Hotline 1.9 clients
  require a slightly different handshake reply or format for a particular
  transaction, detect their version in the Preamble and adjust accordingly.
  SynHX might advertise its own sub-version number; the server can maintain a
  mapping or conditional code paths for known client versions. All such
  differences should fall back to the Hotline 1.9 baseline so that any
  unrecognised client version (including SynHX, which aims to be compatible) at
  least gets standard 1.8.5/1.9 protocol handling. In essence, *if a feature is
  unsupported by a client, the server should degrade gracefully to the older
  behaviour*.

- **Extensibility for Newer Protocols**: While out of scope for this initial
  migration, keep the design open for future protocol extensions. SynHX might
  eventually support features beyond 1.9; the `wireframe` routing approach
  makes it easier to add new message types or alternate handlers in the future
  without deep changes to the core.

## 6. Testing and Validation

A comprehensive testing strategy will verify that the new `wireframe`-based
server is functionally equivalent to the old implementation and remains
compatible with target clients. Both automated tests and real-world client
trials are recommended:

- **Reuse and Update Unit Tests**: Port the existing `mxd` unit tests to the
  new framework. Critical components like handshake parsing and transaction
  encoding already have tests (e.g. `parse_handshake` tests for valid/invalid
  protocol bytes[^26][^27]). These tests should pass with the new code as well
  – if the internal APIs changed (for example, handshake logic moved into the
  `wireframe` preamble handler), adjust the tests or write new ones targeting
  the new structures. Ensure that all edge cases (bad protocol ID, unsupported
  version, oversized payload, etc.) still produce the expected errors or
  outcomes.

- **Integration Testing**: The project includes integration tests (in
  `mxd/tests/` and the `test-util` harness) that simulate client-server
  interactions. Continue to use these to validate the end-to-end behaviour of
  the new server. For example, tests for rejecting invalid payloads or handling
  multi-packet transactions should still pass[^28][^17]. Start the `wireframe`
  server in the test harness (similar to how `TestServer` in `test-util`
  launches the old server) and run the same battery of integration tests
  against it. All previously working scenarios should remain working – this is
  a strong indicator of success.

- **SynHX Client Harness**: Leverage the existing external client tests. The
  development roadmap notes an “integration validator crate using `hx`” (the
  Synapse Hotline X client)[^29], which presumably can drive a real Hotline
  client session against the server. Use this harness to perform real client
  operations (login, file listing, chat, etc.) against the new server. Because
  SynHX/*shx* is known to implement the Hotline protocol, any mismatches or
  errors observed during these runs will highlight compatibility issues that
  slipped through automated tests. For instance, connect with a SynHX client
  and verify that it can log in and receive the correct welcome message, browse
  files, post news, etc., without errors. This step is crucial for ensuring
  that the `wireframe` implementation is truly faithful to the Hotline protocol
  from a client’s perspective.

- **Performance and Stability**: Although not explicitly requested, it’s wise
  to perform some basic performance regression tests (throughput of file
  transfers, number of simultaneous connections, etc.) to ensure the new
  abstraction doesn’t introduce a bottleneck. `wireframe` is asynchronous and
  should handle concurrency well, but testing under load will confirm that it
  meets expectations. Also monitor resource usage (CPU, memory) in long-running
  tests to catch any potential leaks or inefficiencies introduced during the
  refactor.

- **Cross-Platform Verification**: Since the target platform list includes
  multiple architectures, run the test suite on both x86_64 and arm64 (aarch64)
  systems. The Rust code should behave identically on both, but differences in
  endianness are not an issue here (both architectures are little-endian, and
  the protocol is big-endian by definition[^30]). Still, compiling and
  executing on an aarch64 Linux (e.g. using CI or an emulator) can reveal any
  hidden assumptions (like pointer size or atomic usage) in dependencies.
  Additionally, if resources permit, do a smoke test on FreeBSD and macOS to
  ensure the Tokio networking and `wireframe` stack run smoothly on those OSs.
  This might simply involve running a basic handshake or login test on those
  platforms.

## 7. Deployment and Platform Support

Finally, consider the deployment of the new server and the platforms it will
run on, incorporating the target environment requirements:

- **Linux (Primary Target)**: Optimise and test for Linux x86_64 and aarch64 as
  the main deployment targets. Ensure that continuous integration covers
  building the server on these architectures. Any Linux-specific configurations
  (like systemd service files or Docker images) should be updated to point to
  the new binary. Since Rust and `wireframe` handle most low-level details, no
  significant platform-specific code is expected; the focus is on confirming
  smooth operation in Linux environments (e.g. proper binding to network
  interfaces, handling of OS signals for shutdown, etc.).

- **FreeBSD and macOS (Secondary)**: Although not immediately required, keep an
  eye towards FreeBSD and macOS compatibility. Both OSs should be able to run
  the server given Rust’s cross-platform support. If possible, compile the
  server on these platforms to check for any minor incompatibilities (for
  example, file path differences or dependency build issues). Addressing these
  early will make future expansion easier. Platform-specific integration (like
  launch daemons on macOS or rc scripts on FreeBSD) can be deferred until there
  is a need, but no changes in the core protocol code should be necessary for
  these systems.

- **Deployment Rollout**: When the new implementation is fully tested, deploy
  it as the default server binary. Since breaking changes are acceptable at
  this stage, you can retire the old implementation entirely. Coordinate the
  migration such that any existing test servers or development environments
  switch to the `wireframe` server. Given that there’s effectively “no
  deadline” pressure, favour a **thorough rollout**: run the new server in a
  staging environment if available, gather feedback, and only then replace the
  old server in production usage (if any). This careful approach ensures that
  by the time users rely on it, the server has been vetted with both automated
  tests and real client trials.

By following this plan, the `mxd` project will transition to using `wireframe`
for all protocol-level functionality. The result will be a cleaner, more
maintainable implementation of the Hotline protocol that preserves
compatibility with both **Hotline 1.9** clients and modern **SynHX** clients as
a baseline. This migration not only maintains current capabilities but also
positions the project for easier extension and improvement in the future,
leveraging `wireframe`’s robust framework for any new features or protocols to
come. With comprehensive testing and gradual integration, the new server should
seamlessly replace the old one while keeping client experience and behaviour
consistent[^31].

Sources:

- mxd Protocol Specification and Code – handshake and framing
  details[^32][^9][^6][^7][^17]

- wireframe Framework Documentation – usage of preambles and routing[^5][^19]

- mxd Command and Session Handling – existing logic for commands and
  sessions[^28][^23]

- Project Roadmap – testing and compatibility notes (hx client harness,
  fuzzing, etc.)[^29]

[^1]: <https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/README.md#L10-L18>
[^2]: <https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/README.md#L14-L22>
[^3]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/docs/protocol.md#L23-L29>
[^4]: <https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/src/preamble.rs#L11-L19>
[^5]: <https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/src/server/config/preamble.rs#L72-L75>
[^6]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/protocol.rs#L59-L67>
[^7]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/protocol.rs#L94-L101>
[^8]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/protocol.rs#L30-L38>
[^9]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/docs/protocol.md#L47-L56>
[^10]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/transaction.rs#L76-L84>
[^11]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/transaction.rs#L87-L95>
[^12]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/docs/protocol.md#L53-L56>
[^13]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/docs/protocol.md#L128-L133>
[^14]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/transaction.rs#L70-L78>
[^15]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/transaction.rs#L102-L110>
[^16]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/transaction.rs#L16-L24>
[^17]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/transaction.rs#L154-L161>
[^18]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/commands.rs#L86-L94>
[^19]: <https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/README.md#L36-L44>
[^20]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/commands.rs#L116-L124>
[^21]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/commands.rs#L134-L142>
[^22]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/commands.rs#L184-L191>
[^23]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/handler.rs#L20-L28>
[^24]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/docs/protocol.md#L76-L78>
[^25]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/protocol.rs#L64-L71>
[^26]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/protocol.rs#L108-L116>
[^27]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/protocol.rs#L139-L147>
[^28]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/src/commands.rs#L86-L95>
[^29]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/docs/roadmap.md#L53-L60>
[^30]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/docs/protocol.md#L17-L25>
[^31]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/docs/roadmap.md#L55-L60>
[^32]: <https://github.com/leynos/mxd/blob/2dc6cc9d48dc36b3bb17338f3658c3c6cfb2351d/docs/protocol.md#L21-L29>

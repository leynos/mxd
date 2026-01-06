# Adopting Hexagonal Architecture in the mxd-Wireframe Migration

## Structural Changes to Support Ports and Adapters in Wireframe

To align the `wireframe` integration with Hexagonal Architecture (ports and
adapters), the project must introduce clearer layering and interface
boundaries. First, the **mxd** server’s core business logic should be isolated
from infrastructure concerns. The migration plan already calls for refactoring
**mxd** so that *protocol and domain logic are provided as a library crate*,
separate from any specific I/O or network
code([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L22-L30)).
 A new binary (e.g. `mxd-wireframe-server`) will depend on this domain library
and on
`wireframe`([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L22-L26)).
 This ensures the domain logic (command handling, rules, database interaction)
resides in the inner application core, while the new binary (with `wireframe`)
forms the outer layer responsible for networking.

Next, **wireframe** itself must be structured to cleanly represent the
**“Ports”** through which the domain is accessed and the **“Adapters”** that
implement those ports. In practice, this means `wireframe` should expose
abstract extension points (traits or interfaces) for protocol-specific
behaviors and use those to call into the domain. The planned introduction of a
unified `WireframeProtocol` trait is a key structural change: this trait will
encapsulate all protocol-specific logic in one
interface([2](https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/docs/wireframe-1-0-detailed-development-roadmap.md#L40-L43)).
 Instead of scattering handlers and hooks, the `WireframeProtocol` trait
provides a single coherent port through which the domain logic (Hotline
protocol handling) plugs into `wireframe`. The `wireframe` server can then be
configured with `.with_protocol(MyProtocol)` to inject the domain’s
implementation of these
hooks([2](https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/docs/wireframe-1-0-detailed-development-roadmap.md#L40-L43)).
 This design follows the Hexagonal principle of *inversion of control*: the
domain defines how the outside world should interact (via the
`WireframeProtocol` port implementation), and `wireframe` calls those
interfaces from the outside.

Another structural change is replacing the old in-process networking loop with
`wireframe`’s connection handler and frame parser. The plan explicitly
*deprecates the bespoke frame-handling loop* in favor of letting `wireframe`
manage connections and message
framing([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L40-L43)).
 By doing so, the networking code becomes an **Adapter** module outside the
core. This significantly simplifies the codebase by removing duplicate or
parallel implementations of similar
functionality([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L40-L43)).
 The new `mxd-wireframe-server` binary will initialize a `WireframeServer` (or
`WireframeApp`) that listens for connections and delegates all low-level
communication details to `wireframe` components, instead of the domain doing it
itself.

**Layering the codebase** will involve ensuring that the **domain core (mxd
library)** does not depend on `wireframe` (no direct calls into `wireframe`
APIs from business logic). All interactions should flow through abstract
boundaries. For example, the domain can expose functions or traits for
operations like “authenticate user” or “fetch file list,” and the `wireframe`
layer calls those. Conversely, when the domain needs to send output or trigger
an external action (like broadcasting a message to all clients), it should call
an abstraction that `wireframe` provides (e.g. an outbound messaging interface)
rather than manipulating sockets directly. This decoupling allows independent
development and testing of domain logic, and it matches the roadmap’s
requirement that existing tests pass using the domain logic via the new server
without code
duplication([3](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/roadmap.md#L75-L83)).

In summary, **wireframe must be integrated as the outer adapter layer**, and
any framework-specific constructs should be abstracted behind traits or
interfaces that the domain implements. The domain module becomes the inner
hexagon, focused purely on implementing Hotline protocol rules and business
behavior. This structural approach will fulfill the Hexagonal Architecture’s
goal of making the core logic framework-agnostic and easily pluggable into
different adapters.

## Hotline Protocol Concerns as Adapter Responsibilities

Several concerns in the Hotline Protocol implementation are low-level or
infrastructure-oriented – these should be treated as **Adapters** rather than
part of the core domain logic:

- **Connection Handling and I/O Dispatch:** Managing TCP connections,
  reading/writing bytes, and concurrency is a quintessential adapter
  responsibility. In the new design, `wireframe` takes over listening for
  connections and reading from/writing to
  sockets([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L40-L43)).
   The domain core should not perform socket I/O directly. For example,
  connection acceptance and per-connection loops are handled by `wireframe`’s
  runtime, consistent with Hexagonal principles (the infrastructure “drives”
  the core via defined inputs).

- **Handshake Processing:** The Hotline session-init handshake (12-byte “TRTP”
  header exchange) is handled via `wireframe`’s **preamble**
  mechanism([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L50-L58)).
   The handshake bytes are decoded by an adapter layer into a structured form
  (using a `Preamble` trait implementation) before any domain logic
  runs([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L54-L58)).
   The handshake logic – verifying protocol IDs and versions and sending the
  appropriate reply – can largely be seen as part of the networking protocol
  adapter. Indeed, the plan is to implement handshake handling using
  `wireframe` hooks and only *use the existing mxd logic to format the 8-byte
  reply* and error
  codes([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L70-L78)).
   By treating handshake parsing/validation as an adapter concern, the core
  domain can remain focused on higher-level session initialization outcomes
  (e.g. possibly recording client version or deciding if the connection should
  proceed). The handshake adapter will also enforce timing (e.g. a 5-second
  timeout) and drop misbehaving
  connections([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L82-L88)),
   which are policies best kept out of business logic.

- **Transaction Framing and Fragmentation:** Hotline’s messages use a fixed
  20-byte header and may be split across multiple TCP packets. Parsing and
  assembling these frames is a technical concern that `wireframe` will handle
  via a custom serializer (codec). The migration plan calls for implementing a
  **`wireframe::Serializer`** that knows how to parse the 20-byte transaction
  header and reassemble
  fragments([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L96-L104)
   )(
  [1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L107-L114)).
   This serializer is a classic example of an Infrastructure Adapter: it
  translates raw byte streams into higher-level message objects (and vice
  versa) for the domain. The domain logic should not manually parse byte
  headers or manage fragmentation buffers; `wireframe` will invoke the
  serializer for every incoming chunk and only dispatch complete, reconstructed
  messages to the
  core([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L115-L118)).
   Similarly, outgoing responses will be passed to the serializer which adds
  headers, splits large payloads if needed, and writes to the
  socket([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L119-L125)).
   By confining frame layout knowledge to the adapter, any changes in the wire
  format (e.g. different protocol versions) don’t ripple through domain code.

- **Field Parsing and Encoding:** The Hotline protocol uses a complex parameter
  encoding (variable-length integers, length-prefixed strings, XOR-obfuscated
  text for certain fields, etc.). Converting these binary fields into usable
  data types is an adapter responsibility. In the new design, when a request
  reaches a handler, one of the first steps is to *parse the request payload
  into high-level parameters*, often using helper
  functions([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L159-L167)).
   Those helpers (e.g. `decode_params_map`, `first_param_string`) were part of
  mxd’s original implementation and deal with low-level byte
  interpretation([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L159-L167)).
   Under Hexagonal Architecture, this parsing can be seen as part of the input
  adapter: translating the external representation into domain-friendly values.
  The roadmap explicitly notes that certain quirks like XOR-encoded strings
  should be *“detected and decoded… early in request handling so that all
  business logic operates on real plaintext
  values.”*([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L202-L209)).
   Doing this transparently in the adapter layer means the core logic doesn’t
  even need to know a field was XOR-obfuscated on the wire; it simply receives
  a normal string. Likewise, when sending responses, the adapter should encode
  fields (apply XOR for legacy clients, choose 2-byte vs 4-byte integer
  representation, etc.) based on the client’s
  needs([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L202-L209)).
   These tasks are protocol mechanics – treating them as adapter functions
  keeps the domain logic (like “update user password”) free of binary encoding
  details.

- **Error Codes and Response Framing:** In Hotline, errors and replies carry
  specific numeric codes and must echo the original transaction ID. While the
  decision to return an error (e.g. “user not authorized”) is a business
  decision, the mapping of that outcome to a protocol error code and formatted
  reply is an adapter’s job. The plan includes creating a reply builder that
  *“mirrors Hotline error propagation and logging
  conventions”*([3](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/roadmap.md#L86-L89)),
   ensuring that when the domain reports an error or exception, the adapter
  translates it into the correct wire format (setting the error code field in
  the header, copying the request’s transaction ID, etc.). This can be achieved
  by the adapter layer using domain-provided information (like an error type or
  message) to look up or construct the appropriate `Transaction` reply with the
  error code. The domain should not manually assemble the 20-byte header or
  fiddle with error bitflags; those are mechanical details best kept in the
  infrastructure layer (perhaps via the serializer or a response helper).

In essence, **anything related to bytes, buffers, network transport, or
protocol-specific encoding is an Adapter concern**. The `wireframe` framework
is explicitly designed to handle *“manual data serialization and
deserialization, intricate framing logic, stateful connection management, and
the dispatch of messages to handlers”*, all of which are described as
*“low-level concerns” that would otherwise *“obscure the core application
logic”*(
[4](https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/docs/rust-binary-router-library-design.md#L5-L13)).

 By offloading these concerns to `wireframe`, the migration ensures that mxd’s
 core logic can operate at the level of parsed messages and domain concepts
 (users, files, chats) rather than byte streams and socket states.

## Ports vs. Application Core in the Wireframe Integration

Under Hexagonal Architecture, a **“Port”** is an interface that defines how the
outside world interacts with the core, while the **“Application Core”**
contains the domain logic that implements those interactions. In the context of
the mxd wireframe adoption, we need to distinguish which parts of the system
act as ports (interfaces or entry points) and which parts belong to the domain
core.

**Inbound Ports (Driving)**: For the Hotline server, the inbound port is
essentially the set of operations the server can perform in response to client
requests – login, file listing, chat message, etc. In the new design, this is
represented by the **message routing interface** provided by `wireframe`. Each
Hotline *transaction type* (identified by an ID like 0x006B for Login, 0x006C
for GetFileList, etc.) can be viewed as an inbound use-case in the domain. The
`wireframe` library allows registering routes for each message type, mapping
them to handler
functions([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L148-L156)).
 These route definitions (and ultimately the `WireframeProtocol` trait
implementation) serve as the **Port interface**: they declare “When a message
of type X arrives, it will be handled by function Y.” The handlers themselves
are the implementation of the port, calling into domain logic. In other words,
the combination of message type definitions and the `wireframe` routing
configuration constitutes a port through which the network drives the
application core.

The **Application Core** is the existing mxd domain logic – all the code that
actually processes requests and embodies the rules of the Hotline protocol and
server behavior. This includes things like validating a user’s credentials,
looking up files in the database, updating news posts, enforcing permissions,
etc. The migration plan makes it clear that these routines (e.g.
`handle_login`, `list_files_for_user`, etc.) should remain in the mxd library
and be *reused* by the new handlers.[^migration-plan-bridge] Thus, when a
`Login` message comes in through the port (wireframe route), the handler will
*“bridge to the existing mxd logic”*.[^migration-plan-bridge] – effectively
invoking the core application logic to do the work. The core returns a result
(e.g. login success or failure, plus any data), and the handler then wraps that
into a response object. By structuring it this way, **the domain logic is
invoked via an interface (the handler/port) and does not itself depend on the
transport**. The domain doesn’t know or care that the request came from a TCP
socket or that the response will be encoded in a certain binary format – those
details are abstracted by the port adapter.

**Outbound Ports (Driven)**: The server may also need to initiate actions
toward clients asynchronously (for example, broadcasting a “user X has logged
in” notification to others, or pushing periodic heartbeats). In Hexagonal
terms, this is an outbound port: the core needs a way to send messages out. The
`wireframe` library provides a mechanism for this via its **push API and
session registry** (for finding
connections)([2](https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/docs/wireframe-1-0-detailed-development-roadmap.md#L40-L43)
 )(
[2](https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/docs/wireframe-1-0-detailed-development-roadmap.md#L42-L43)).
 Conceptually, we can model this as a port interface like “ClientNotifier” with
methods to deliver certain events (e.g. `notifyUserJoined(sessionInfo)` or more
generically `pushMessage(connectionId, frame)`). The implementation of this
port is the adapter that uses `wireframe`’s `PushHandle` and `SessionRegistry`
to actually deliver the frames. The roadmap indeed includes tasks to implement
a `SessionRegistry` for discovering connection
handles([2](https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/docs/wireframe-1-0-detailed-development-roadmap.md#L40-L43))
 and a public `PushHandle` API to send outbound
frames([2](https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/docs/wireframe-1-0-detailed-development-roadmap.md#L40-L43)).
 These will act as the **adapter** enabling outbound communication. The domain
core should use them via an abstract interface. For example, after processing a
login, the core logic might call `Notifier.broadcastUserLogin(user)` – behind
that, the adapter will use `wireframe` to send a “Notify New User” (transaction
300) to all connected clients. By treating outbound messaging as a port, the
domain remains unaware of *how* the broadcast is done (whether via `wireframe`
channels, websockets, or any other mechanism – it could be swapped without
altering core logic).

**Domain Interfaces vs. Wireframe Interfaces**: It’s important to clarify which
interfaces belong to the domain (“ports” defined by the core) and which are
provided by `wireframe`. In a pure Hexagonal approach, one would define all
necessary port interfaces inside the core and then implement them in the outer
layer. In our case, some interfaces are implicitly provided by `wireframe`
(e.g. the routing mechanism, session management). The key is to use them in a
way that preserves the core’s independence. For inbound interactions, the
domain could define an interface like `HotlineRequestHandler` with methods for
each transaction type – but since Rust allows us to pass function pointers or
closures, the domain logic can be connected to `wireframe` routes without a
formal trait for each message. The effect is similar: the `wireframe` route is
the port, and the domain function is plugged in. The **migration tasks
explicitly state**: *“Map every implemented transaction ID to a wireframe route
that delegates to the existing domain
handlers.”*([3](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/roadmap.md#L75-L83)).
 This clearly separates the responsibilities: the mapping (in wireframe) is the
port configuration, and the domain handler is the core execution.

Similarly, for outbound communications, the domain will rely on an abstraction
to send messages. The introduction of the push API means the domain should
obtain a `PushHandle` (or through a higher-level service) to emit events
instead of manually managing sockets. In a Hexagonal model, the domain might
not even know about `PushHandle` – it might call a method on a domain service,
which internally uses the push adapter. In practice, since `wireframe` is
tightly integrated, the domain might directly call something like
`session.push(frame)` if not careful. To keep with the Hexagon, those calls
should be wrapped or isolated. The new `WireframeProtocol` trait may help here:
one of its responsibilities during connection setup is to attach any
per-connection state, which could include a handle or context that domain logic
uses when
needed([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L176-L184)).
 For example, the domain’s session object (tracking user ID, etc.) might hold a
reference to a sender that allows pushing to that client. Because
`WireframeProtocol` (a port interface) mediates this, the domain isn’t
explicitly depending on the lower-level details – it just calls a method on its
context, which the adapter fulfills by sending through `wireframe`.

In summary, **the elements of the `wireframe` integration that should be
considered “Ports” are those that interface with the domain**: the routing of
incoming messages (input port) and the facility to send out messages or access
infrastructure services (output ports, like notification or persistence if
any). The **Application Core** encompasses the logic in the mxd library –
parsing command parameters into meaningful data, applying business rules,
updating state, and deciding what responses or events are appropriate. The core
should expose clear entry points (which `wireframe` calls via handlers) and
consume abstracted services (which `wireframe` provides via adapters). By
delineating these, we ensure that `wireframe` is a plug-in mechanism rather
than a fundamental dependency tangled through the core code.

## Avoiding Hexagonal Violations and Addressing Risk Areas

The migration plan and roadmap are largely aligned with a ports-and-adapters
style, but a few areas require vigilance to avoid breaking the separation of
concerns:

- **Mixing Protocol Mechanics with Business Logic:** One risk is that some
  low-level protocol details might leak into the domain code or vice versa. For
  example, the plan suggests using existing mxd utilities like `reply_header`
  and `encode_params` to construct
  responses([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L170-L178)).
   While reusing these helps avoid duplication, it means domain code is
  directly formatting network frames (headers and parameter bytes). In a pure
  Hexagonal approach, the domain would produce a high-level result (e.g. a data
  structure or an error type), and the adapter would handle converting that
  into the proper reply frame. To address this, over time those helper
  functions can be migrated or wrapped into the `wireframe` layer (for
  instance, as part of the Serializer or a response builder). In the interim,
  it’s acceptable to call them from the handler (since the handler is
  essentially part of the adapter layer, mediating between domain and network).
  The key is to **limit such formatting code to the adapter/handler, and not
  inside the core domain routines**. If any domain function currently builds a
  byte buffer to send, that should be refactored so the domain function returns
  structured data and the sending is done by the outer layer.

- **Domain Logic Calling Framework Code Directly:** With `wireframe` providing
  powerful features (like pushing messages, managing sessions), there is a
  temptation to call those APIs from anywhere. Doing so in domain code would
  violate Hexagonal layering by making the core depend on an external
  framework. For instance, if the `mxd` core had a function that, upon certain
  conditions, calls `WireframeApp::broadcast()` or uses a `PushHandle`
  globally, that would couple the core to `wireframe`. The solution is to
  insert an abstraction: domain code can signal an event or call an interface
  (like a domain service) which the `wireframe` adapter listens to or invokes.
  Concretely, for broadcasting presence or chat notifications, the domain could
  call something like `SessionManager.notifyAll(event)` – an interface that is
  implemented in the outer layer using `wireframe` push mechanisms. The
  roadmap’s introduction of a shared session context and push
  API([3](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/roadmap.md#L80-L88)
   )(
  [2](https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/docs/wireframe-1-0-detailed-development-roadmap.md#L40-L43))
   hints at how to do this cleanly. By storing a `PushHandle` or connection
  reference in the session context (attached via `WireframeProtocol` when a
  user
  connects)([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L176-L184)),
   the domain can remain unaware of the specifics – it might just call
  `session.send(message)` on its session object, which internally uses the
  adapter. Ensuring **the domain never reaches out into the `wireframe` layer
  directly** keeps the dependency one-way (framework -> domain for inbound, and
  domain -> abstraction -> framework for outbound).

- **Session State and Context Bleed-through:** Managing session state is
  inherently a cross-cutting concern – it lives partly in the domain (the
  concept of a logged-in user with permissions) and partly in the
  infrastructure (data stored per connection). The plan is to *“preserve
  per-connection state”* by utilizing `wireframe`’s session/context
  features([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L176-L184)).
   The potential pitfall here is if the domain has to *reach into `wireframe`
  internals to get session info*. This should be avoided by designing a clean
  interface for session access. The migration suggests implementing connection
  initialization via `WireframeProtocol` to attach a `Session` object to each
  connection(
  [1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L176-L184)).
   That `Session` struct is a domain construct (carrying user ID, etc.), but it
  can be stored in a way that handlers can retrieve it easily (for example,
  `wireframe` could provide a method to get the session for the current
  connection, returning a domain `Session`). By doing this, when a domain
  handler needs to check authentication or update user state, it calls a method
  to get its session (or it might be passed in the handler’s context), rather
  than poking at a global map or `wireframe` types. The risk of violation is
  low if the design follows the plan: the domain will treat session as just
  another piece of data, while `wireframe` ensures each handler has the correct
  session attached. To be safe, **document clearly which part of session
  management is domain (contents of the Session struct, business rules for
  session) and which is adapter (storage and lookup of session per
  connection)**. This prevents confusion and accidental misuse of one layer’s
  data structures in the other.

- **Handling of Compatibility Quirks:** The Hotline protocol has various
  version-specific behaviors and encoding quirks. The plan addresses these by
  gating them on the handshake metadata (sub-version) and performing
  adjustments at the
  edges([3](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/roadmap.md#L99-L103)
   )(
  [1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L214-L222)).
   A potential mistake would be implementing these conditions deep in the
  domain logic in scattered places, which can clutter core code with
  protocol-version checks. Instead, these should be handled either in a
  centralized policy module or at the adapter level. For example, if a certain
  client version expects a slightly different handshake reply format, the
  handshake adapter code (which knows the client sub-version from the preamble)
  can choose the format before invoking the core (or when formulating the
  reply). If an older client doesn’t support a new field, the adapter could
  strip or translate that field on output. The roadmap explicitly says to
  *“detect clients that XOR-encode text fields and transparently decode or
  encode responses when required”*.[^roadmap-xor] – the word *“transparently”*
  underscores that the domain use-case (say, setting a password) shouldn’t need
  an `if client_version < X { ... }` inside it. The transparency is achieved by
  the adapter doing the encoding/decoding around the core. By following those
  guidelines, the implementation remains compliant with Hexagonal Architecture:
  new client variations are handled by adapter strategies, and the domain logic
  stays consistent and focused on core behavior.

- **Testing and Temporary Duality:** During migration, there may be a period
  where both the old path and new path exist (controlled by feature
  flags)([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L40-L43)
   )(
  [3](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/roadmap.md#L25-L29)).
   It’s important that this transitional state does not introduce hidden
  coupling. For instance, tests might run through the old path and new path and
  compare results. As long as the domain logic is singular (extracted to the
  library) and both old and new use it, this is fine. The caution is to ensure
  the old networking code is fully isolated and will be removed. Having two
  adapters (legacy loop and `wireframe`) is acceptable short-term, but no
  domain code should be specifically aware of which is in use – it should
  produce the same outcomes regardless. The acceptance criteria that tests pass
  unchanged against the new
  server([3](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/roadmap.md#L75-L83))
   is a good indicator that the hexagonal boundaries are correct: the core
  produces the same observable behavior no matter the adapter driving it.

In conclusion, any area where the **domain starts to know too much about “how”
things are done** (network protocol details, specific framework types, etc.) is
a red flag. The remedy is always to introduce an intermediary boundary: a
function call, a trait, a context object, etc., that cleanly separates the
concern. The mxd-wireframe migration plan is cognizant of this – it repeatedly
emphasizes reusing domain handlers and keeping the new framework code at the
edges(
[1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L164-L171)).
 By continuing to enforce a one-way dependency (wireframe depends on domain
library; domain logic does not depend on wireframe), the project will adhere to
Hexagonal Architecture. Any deviations (like a domain function manipulating a
`PushHandle` or constructing a raw frame) should be refactored so that either
the operation is done in the adapter layer or it’s abstracted behind a domain
interface. This disciplined approach will result in a **clean
Ports-and-Adapters structure**: the Hotline server’s core logic will be
isolated and testable, and all interactions with networks, clients, and
databases will happen through well-defined ports and adapter
implementations([1](https://github.com/leynos/mxd/blob/88d1cfb3097b2d96f2b7c9d1382f6b374d7eb90c/docs/migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L28-L31)
 )(
[4](https://github.com/leynos/wireframe/blob/fa6c62925443e6caed54866a95d3396eb8fa78a2/docs/rust-binary-router-library-design.md#L5-L13)).

[^migration-plan-bridge]:
  migration-plan-moving-mxd-protocol-implementation-to-wireframe.md#L164-L168
[^roadmap-xor]:
  roadmap.md#L95-L103

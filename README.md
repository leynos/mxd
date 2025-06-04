# mxd
Marrakesh Express Daemon

mxd is a minimal implementation of a [Hotline](https://hotline.fandom.com/wiki/Virtual1%27s_Hotline_Server_Protocol_Guide) server written in Rust.
It currently implements the bare essentials for accepting TCP connections and
authenticating users stored in a SQLite database using Diesel with its async
extension. Passwords are stored as SHA-256 hashes.
Commands are read line by line using Tokio's `BufReader` and a simple `LOGIN`
command is supported. Invalid `LOGIN` requests result in an `ERR` response.
Each client session stays open so multiple commands can be processed until the
client disconnects.

The server is asynchronous thanks to Tokio and is intended as a starting point
for a more complete implementation of the Hotline protocol.

## Running

Build the project then run the daemon specifying the bind address and database p
ath if desired:

```
cargo build

# Run server listening on the default address
cargo run -- --bind 0.0.0.0:5500 --database mxd.db
```

### Creating users

Use the `create-user` subcommand to add accounts:

```
cargo run -- create-user alice secret
```

### Running tests

```
cargo test
```


## Validation harness

A separate crate named `validator` provides integration tests using the
`shx` client and the `expectrl` crate. Install `shx` version 0.2.4 and ensure it
is on your `PATH` before running the tests:

```bash
cd validator
cargo test
```

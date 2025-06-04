# mxd
Marrakesh Express Daemon

mxd is a minimal implementation of a [Hotline](https://hotline.fandom.com/wiki/Virtual1%27s_Hotline_Server_Protocol_Guide) server written in Rust.
It currently implements the bare essentials for accepting TCP connections and
authenticating users stored in a SQLite database using Diesel as an ORM.

The server is asynchronous thanks to Tokio and is intended as a starting point
for a more complete implementation of the Hotline protocol.

## Running

```
# Build
cargo build

# Run server listening on 0.0.0.0:5500 with default database mxd.db
cargo run
```

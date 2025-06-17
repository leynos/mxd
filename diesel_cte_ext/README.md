# diesel_cte_ext

`diesel_cte_ext` adds a small helper for building recursive
[Common Table Expressions](https://www.postgresql.org/docs/current/queries-with.html#QUERIES-WITH-RECURSIVE)
with Diesel. The crate exports a connection extension trait providing
`with_recursive` which constructs a query representing a `WITH RECURSIVE`
block.

```rust
use diesel::dsl::sql;
use diesel::sql_types::Integer;
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};
// Count integers from 1 through 5 using a recursive CTE

let rows: Vec<i32> = conn
    .with_recursive(
        "t",
        &["n"],
        RecursiveParts::new(
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
            sql::<Integer>("SELECT n FROM t"),
        ),
    )
    .load(&mut conn)?;
```

The resulting CTE `t` contains the following rows:

| n |
|---|
| 1 |
| 2 |
| 3 |
| ... |

When `diesel-async` is enabled, import `diesel_async::RunQueryDsl` and await the
query as follows:

```rust
use diesel::dsl::sql;
use diesel::sql_types::Integer;
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};
use diesel_async::RunQueryDsl;

let rows: Vec<i32> = conn
    .with_recursive(
        "t",
        &["n"],
        RecursiveParts::new(
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
            sql::<Integer>("SELECT n FROM t"),
        ),
    )
    .load(&mut conn)
    .await?;
```

The builder works with either SQLite or PostgreSQL depending on the enabled
Cargo feature. It can be used with synchronous or asynchronous Diesel
connections.

## Capabilities

- Construct a single recursive CTE with a seed query, step query and body.
- Tested with both SQLite and PostgreSQL back ends.
- Compatible with Diesel 2.x synchronous and `diesel-async` connections.

## Limitations

- Only supports a single CTE block and requires manually listing column names.
- No integration with Diesel's query DSL or schema inference.
- Crate is unpublished and APIs may change without notice.

## Next steps

Future improvements could include typed column support, better integration with
Diesel's query builder, and support for multiple chained CTEs.

## Caveats

This crate is experimental. Error handling is minimal and the API may evolve.
Use at your own risk.

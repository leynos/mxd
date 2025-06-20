# diesel_cte_ext Design

diesel_cte_ext provides small helpers for building Common Table Expressions
(CTEs) with Diesel. The crate exposes builders for both recursive and
non-recursive CTE blocks. A connection extension trait makes those builders
available on synchronous and asynchronous connections.

## Architecture

```mermaid
classDiagram
    class WithCte {
        +ctename: &'static str
        +columns: Columns<Cols>
        +cte: Cte
        +body: Body
        +_marker: PhantomData<DB>
    }
    WithCte <|.. QueryId
    WithCte <|.. Query
    WithCte <|.. QueryFragment
    WithCte <|.. RunQueryDsl

    class RecursiveCTEExt {
        <<trait>>
        +with_recursive(...): ...
        +with_cte(...): WithCte
    }
    RecursiveCTEExt <|.. SqliteConnection
    RecursiveCTEExt <|.. AsyncConnection

    class builders {
        +with_recursive(...)
        +with_cte(...): WithCte
    }

    class Columns {
        <<generic>>
    }

    class SqliteConnection
    class AsyncConnection

    WithCte o-- Columns
    RecursiveCTEExt ..> WithCte : returns
    builders ..> WithCte : returns
    RecursiveCTEExt ..> builders : uses
    SqliteConnection ..|> RecursiveCTEExt
    AsyncConnection ..|> RecursiveCTEExt
```

The `WithCte` type stores the CTE name, columns, and query fragments for the
common table expression. The builders produce a `WithCte` instance that can be
executed using Diesel's `RunQueryDsl`.

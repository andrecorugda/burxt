# The Burxt guide

Twelve pages, in reading order. Each one explains a decision and the reasoning behind it —
what the language refuses is usually the interesting part.

| | Page | |
|---|---|---|
| 1 | [Getting started](01-getting-started.md) | Install, run a file, the editor |
| 2 | [Numbers and money](02-numbers-and-money.md) | Scales, rounding contracts, why `+` is strict |
| 3 | [Types](03-types.md) | Classes, `private`, constructors, interfaces, enums, value semantics |
| 4 | [Memory](04-memory.md) | Regions, escapes, and why you never write `allocates` |
| 5 | [Contracts](05-contracts.md) | `requires`, `ensures`, `pure`, `decreases`, `old` |
| 6 | [Effects](06-effects.md) | `touches files, network` — what a function can reach |
| 7 | [The C boundary](07-ffi.md) | `external function`, `as scaled`, the pointer wall |
| 8 | [Modules](08-modules.md) | `use`, one file per module, what is visible |
| 9 | [Generics](09-generics.md) | Type parameters, bounds, why nothing is erased |
| 10 | [Absence and failure](10-absence-and-failure.md) | `Option`, `Result`, `?`, and no null |
| 11 | [Maps and strings](11-maps.md) | Insertion order, `Equatable` keys, bytes |
| 12 | [Tools and agents](12-tools-and-agents.md) | `burxt mcp-schema`, `burxt review` — the contract IS the tool schema |

The reference moved out of this directory and is now **generated**: `scripts/site-reference.py`
reads the keyword table out of `src/lexer.rs`, the reserved names out of `src/typeck.rs`, the
commands out of `src/main.rs` and every standard-library entry out of the `//` prose in `lib/*.bx`.
It lives in [`docs/reference/`](../reference/index.md). The hand-written page it replaces had gone
stale — it still listed `record`, renamed to `class` eleven versions earlier.

Running code beats prose: [`../../examples/`](../../examples) has one program per idea, and
every one of them compiles.

# The Burxt guide

Eleven pages, in reading order. Each one explains a decision and the reasoning behind it —
what the language refuses is usually the interesting part.

| | Page | |
|---|---|---|
| 1 | [Getting started](01-getting-started.md) | Install, run a file, the editor |
| 2 | [Numbers and money](02-numbers-and-money.md) | Scales, rounding contracts, why `+` is strict |
| 3 | [Types](03-types.md) | Records, enums, traits, `dynamic`, value semantics |
| 4 | [Memory](04-memory.md) | Regions, `allocates`, escapes |
| 5 | [Contracts](05-contracts.md) | `requires`, `ensures`, `pure`, `decreases`, `old` |
| 6 | [The C boundary](06-ffi.md) | `external function`, `as scaled`, the pointer wall |
| 7 | [Modules](07-modules.md) | `use`, one file per module, what is visible |
| 8 | [Generics](08-generics.md) | Type parameters, bounds, why nothing is erased |
| 9 | [Absence and failure](09-absence-and-failure.md) | `Option`, `Result`, `?`, and no null |
| 10 | [Maps](10-maps.md) | Insertion order, `Equatable` keys, `get` and `find` |
| — | [Reference](reference.md) | Every keyword, builtin, operator and error |

Running code beats prose: [`../../examples/`](../../examples) has one program per idea, and
every one of them compiles.

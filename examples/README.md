# Examples

Every file here compiles. Run one with:

```sh
burxt run examples/invoice.bx -o /tmp/invoice
```

## Start here

| File | What it teaches |
|---|---|
| [`tour.bx`](tour.bx) | The whole language in 68 lines — money, records, interfaces, enums, tail calls, regions, FFI. Read this first. |
| [`invoice.bx`](invoice.bx) | The program someone would actually write: line items, a tax rate, exact totals, and a contract on the tax function. |
| [`money.bx`](money.bx) | The five lines that prove the thesis. |

## One idea at a time

| File | What it teaches |
|---|---|
| [`inference.bx`](inference.bx) | `let x = e;` — where the type may be left out, where it may not, and why an array is the exception. |
| [`absence.bx`](absence.bx) | No null: `Option<T>` and `Result<T, E>` as library types, and the compiler making you handle both cases. |
| [`generics.bx`](generics.bx) | One definition, one copy per type: monomorphisation, inference at the call site, and what an unbounded parameter may not do. |
| [`interfaces.bx`](interfaces.bx) | Interfaces, `implement Interface for Type`, static dispatch, `dynamic` — and why there is no inheritance. |
| [`enums.bx`](enums.bx) | Variants with payloads, exhaustive `match` with no wildcard, and what a new variant breaks. |
| [`regions.bx`](regions.bx) | Regions as the unit of ownership, `allocates`, and every escape the compiler refuses. |
| [`services.bx`](services.bx) | **Coming from classes**: a constructor with validation, an interface, dependency injection through a `dynamic` field, and two implementations swapped with no change to the service. |
| [`contracts.bx`](contracts.bx) | `requires` / `ensures` / `pure` / `decreases` / `old(...)`, including a conservation law. |
| [`ffi.bx`](ffi.bx) + [`ffi.c`](ffi.c) | The C boundary, the `as scaled` marshaller, and the pointer wall. Needs the `.c` file linked — see the header. |
| [`order.bx`](order.bx) | A small order calculation. |

Each of these ends with a **"what the compiler refuses, and why"** section, quoting the
real error text. The refusals are the language: what a compiler declines to compile says
more about it than what it accepts.

## The compiler, in Burxt

| File | What it is |
|---|---|
| [`stage1.bx`](stage1.bx) | **The Burxt compiler, written in Burxt** — the program, 105 lines, which `use`s the five modules below. It compiles its own source to a byte-identical fixpoint. |
| [`burxt/types.bx`](burxt/types.bx) | The shapes everything else is written in, and the lexer. |
| [`burxt/parser.bx`](burxt/parser.bx) | Tokens in, arena AST out. |
| [`burxt/check.bx`](burxt/check.bx) | The rules: scales, regions, purity, contracts, exhaustiveness. |
| [`burxt/modules.bx`](burxt/modules.bx) | `use "path";`, resolved before lexing. |
| [`burxt/emit.bx`](burxt/emit.bx) | Textual LLVM IR, and the runtime emitted with it. |
| [`lexer.bx`](lexer.bx) | The lexer alone, on a file with a byte it does not know. |
| [`parser.bx`](parser.bx) | The parser alone, building an arena AST. |
| [`checker.bx`](checker.bx) | The scale rule, in Burxt, on a file with real mistakes in it. |
| [`symbols.bx`](symbols.bx) | A symbol table catching a redeclaration. |

These read their inputs from [`inputs/`](inputs) (valid Burxt) and
[`negative/`](negative) (**deliberately** wrong — that is what they exist to catch).

## The same app, four times

A point-of-sale — a fixed catalogue, one sale, two tax rules, one receipt — written four
times, split the same four ways every time. Same module boundaries, same function names,
same order inside each file, so they can be read side by side.

| | |
|---|---|
| [`pos/`](pos) | **Burxt.** `items.bx` · `tax.bx` · `receipt.bx` · `till.bx` — and [the four-way comparison](pos/README.md) |
| [`pos-python/`](pos-python) | Python, with `decimal.Decimal` and an explicit `ROUND_HALF_UP` per operation |
| [`pos-php/`](pos-php) | PHP, with bcmath strings — every arithmetic op is a call, and half-up is hand-rolled |
| [`pos-rust/`](pos-rust) | Rust, with `i64` cents in a newtype — there is no decimal in its standard library |

All four print the same receipt. Writing them found a **wrong answer in money** in the Burxt
compiler that 36 invariants had missed, which is the argument for the exercise: three
independent implementations agreeing with each other and not with you is hard to argue with.

## Something you would actually deploy

| | |
|---|---|
| [`mcp/`](mcp/README.md) | **An MCP server** — JSON-RPC over stdio, two money tools, and `burxt mcp-schema` deriving the tool schema from the preconditions so it cannot drift from the implementation |

That last part is the point, and it is the one thing here no other language can do: everywhere else
the JSON Schema a client validates against and the check the function performs are two artifacts
maintained by hand, and the schema is the one that rots.

## What it refuses

| | |
|---|---|
| [`refused/`](refused/README.md) | **Ten mistakes that compile in every other language**, each with the compiler's exact words |

Read that one asking honestly which you would have caught in a pull request at 5pm. `pos/` shows
the money is exact; this shows the part that matters more — every refusal there is a review nobody
has to do. Generated by running the compiler, and a test diffs it, so the page cannot claim a
refusal the compiler does not make.

## Learning the language

The guide in [`../docs/guide/`](../docs/guide) is the prose version of all of this, in
reading order, with the reasoning behind each decision.

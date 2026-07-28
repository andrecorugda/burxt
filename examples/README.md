# Examples

Every file here compiles. Run one with:

```sh
burxt run examples/invoice.bx -o /tmp/invoice
```

## Start here

| File | What it teaches |
|---|---|
| [`tour.bx`](tour.bx) | The whole language in 68 lines — money, structs, traits, enums, tail calls, regions, FFI. Read this first. |
| [`invoice.bx`](invoice.bx) | The program someone would actually write: line items, a tax rate, exact totals, and a contract on the tax function. |
| [`money.bx`](money.bx) | The five lines that prove the thesis. |

## One idea at a time

| File | What it teaches |
|---|---|
| [`traits.bx`](traits.bx) | Traits, `impl Trait for Type`, static dispatch, `dyn` — and why there is no inheritance. |
| [`enums.bx`](enums.bx) | Variants with payloads, exhaustive `match` with no wildcard, and what a new variant breaks. |
| [`regions.bx`](regions.bx) | Regions as the unit of ownership, `allocates`, and every escape the compiler refuses. |
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

## Learning the language

The guide in [`../docs/guide/`](../docs/guide) is the prose version of all of this, in
reading order, with the reasoning behind each decision.

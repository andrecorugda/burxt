<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/burxt-lockup-dark.png">
  <img src="assets/burxt-lockup-light.png" alt="Burxt" width="300">
</picture>

**A typed, compiled, native language where exact decimals are the default and correctness is enforced by the compiler — not left to discipline.**

Burxt compiles to native machine code through LLVM. It is built around one conviction: the dangerous defaults that cause real bugs — binary floats for money, silent overflow, null, implicit coercion — should be *compile errors*, not habits you have to remember to avoid.

```burxt
let price: Decimal<2> = 19.99;
let qty:   Int        = 3;
let total: Decimal<2> = price * qty;
print(total);            // 59.97 — exact, computed as scaled integers, no float anywhere
```

That `59.97` is not a rounded float. Burxt never puts money through binary floating point; a `Decimal<2>` carries its precision in its type and is represented exactly. `0.1 + 0.2` is `0.3`, always.

And where a result *could* round, the compiler makes you say how:

```burxt
let rate:    Decimal<2>                  = 1.01;
let balance: Decimal<2>                  = 1000.00;
let total:   Decimal<2>                  = balance * rate;   // compile error
```
> ``error: `*` on Decimal<2> needs an explicit rounding contract, because the exact result can have more than 2 decimal places. Declare one in the type, e.g. Decimal<2, RoundHalfEven> or Decimal<2, RoundHalfUp>.``

Name the contract and it compiles — and the contract is part of the type, so it travels with the value:

```burxt
let rate:    Decimal<2, RoundHalfEven> = 1.01;
let balance: Decimal<2, RoundHalfEven> = 1000.00;
print(balance * rate);                 // 1010.00 — rounded exactly as declared
```

## Why Burxt exists

Banks still run COBOL for one honest reason: it had exact decimal arithmetic as a first-class type in 1959, and most modern languages still don't — they make you reach for a library and remember to use it. Burxt makes exactness the default and lifts money-correctness into the type system, so the compiler catches the mistake instead of the auditor.

The same principle generalizes. Burxt's identity is: **the compiler refuses to let a silent, dangerous thing happen.**

- **Exact decimals by default** — money is base-10 and exact; precision lives in the type (`Decimal<2>`, `Decimal<4>`), and a rounding contract (`Decimal<2, RoundHalfEven>`) is required before any operation that could round.
- **No silent surprises** — integer overflow traps instead of wrapping, no implicit or lossy conversions, one equality with no coercion, immutable by default, array bounds always checked, and shadowing refused.
- **Composition-first OOP** — small traits, explicit `impl Trait for Type` conformance, static dispatch by default and runtime dispatch only where you write `dyn`.
- **Native, cross-platform by design** — one LLVM backend, many targets: desktop, mobile, and web (WebAssembly). The front end knows nothing about any platform, so reach is a configuration problem rather than a rewrite.

## Status

Burxt is early and built in small, verified increments. The numeric core is solid and the object model is taking shape. It is **not yet ready for production use** — it is ready to watch, try, and shape.

**Burxt compiles Burxt.** As of v0.0.73 the compiler is written in Burxt — lexer, parser, typechecker and an LLVM-IR backend, 4,900 lines of it — and it compiles its own source. The compiler *it* produces emits **byte-identical** output for that same source, which is the fixpoint that says the two implementations agree about the whole language rather than about the programs someone thought to test. A test runs the chain on every commit.

The honest scope, because it matters: the Burxt backend does not emit Decimals, `match`, `tail` or contracts yet — none of which its own source uses — and anything it cannot lower is refused by name rather than emitted wrongly. The Rust compiler stays as the trust anchor and as a differential test, so a change to the language now has two implementations that must agree. Details in [`spec/M4-SELF-HOSTING.md`](spec/M4-SELF-HOSTING.md).

**Working today:**

- **Numbers.** Exact decimals with explicit rounding contracts (`RoundHalfEven` / `RoundHalfUp`), money and percent literals (`$19.99`, `8.25%`), overflow-trapping checked arithmetic, i128 intermediates so the overflow error means the *result* does not fit.
- **The basics.** Integers, booleans, strings (length, byte access, equality, concatenation, interpolation both as a print and as a value), `let` / `let mut`, functions, recursion, `if` / `else if` / `else`, `while`, `&&` / `||` / `!` with real short-circuiting.
- **Types.** Nominal structs with value semantics, fixed-size and growable arrays with always-on bounds checks, sum types with **exhaustive `match`** (no wildcard, so a new variant breaks every incomplete match), methods with value or mutating receivers, traits with static dispatch by default and `dyn` fat-pointer dispatch only where written.
- **Memory.** **Regions** as the unit of ownership: a bump allocator, release in O(1), no GC and no refcounts, and compile-time escape checking that refuses returning region storage. Single-owner regions are what make data-race freedom reachable without per-object borrow checking.
- **Guaranteed tail calls.** `return tail f(...)` is a *checked* guarantee (LLVM `musttail`), not an invisible optimization: 50 million frames in constant stack, or a compile error explaining why the guarantee cannot be given.
- **The C boundary.** `extern fn`, plus **exactness that survives it**: a `Decimal` crosses only through a declared marshaller (`amount: Decimal<2> as scaled`), a `Decimal` → C `double` crossing is a compile error, and an `Int` → `double` crossing is range-checked at 2^53.
- **File input.** `read_file` and `to_string`, the two things a self-hosted compiler cannot do without.
- **Contracts, checked.** `requires` / `ensures` / `decreases` on a signature, checked at runtime with no build mode that strips them, `old(...)` in an `ensures` so a conservation law is expressible (`ensures from + to == old(from + to)`), and `pure` as a compiler-checked claim that a function's answer depends on its arguments alone.
- **Self-hosting.** The whole compiler, in Burxt: a lexer, a parser building an arena AST, a typechecker that agrees with this one on every program in the suite, and an LLVM-IR backend — compiling itself to a fixpoint.

**Designed and committed, not yet built:** no null (absence as an explicit `Option<T>` the compiler forces you to handle); errors as values you must handle; *static proof* of the contracts that are checked at runtime today — the verification layer that is Burxt's eventual differentiator; algebraic effect handlers instead of coloured `async`; and the cross-compilation targets above. Single inheritance was **dropped**, not deferred: composition and traits do the work, and the reasoning is recorded rather than reversed quietly.

`DESIGN.md` records the design north star and a ledger of superseded decisions and deliberately deferred features, each with the trigger that would earn it a milestone. Every version's entry — what it decided and what it cost — is in [`docs/log/`](docs/log/). The distinction between shipped and planned is kept honest in both, on purpose.

## Building

Burxt's compiler (the bootstrap/stage-0 compiler) is written in Rust and emits native code via LLVM 18.

Requirements:
- Rust (via [rustup](https://rustup.rs))
- LLVM 18 development libraries. On Debian/Ubuntu: `llvm-18-dev`, `libpolly-18-dev`, `libzstd-dev`, `clang-18`. On macOS: `brew install llvm@18`.

```sh
# point the LLVM bindings at your LLVM 18 install
export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18     # or $(brew --prefix llvm@18) on macOS

cargo build
./target/debug/burxt run examples/money.bx      # prints 59.97
```

Commands:
```
burxt check   <file.bx>                  parse and typecheck only — no LLVM, no linker
burxt build   <file.bx> [link args...]   compile to a native executable
burxt run     <file.bx> [link args...]   compile, then run
burxt emit-ir <file.bx>                  print the generated LLVM IR
burxt layout  <file.bx>                  print struct layouts (size, alignment, field offsets)
```

Arguments after the source file are passed to the system linker unchanged, so the
C you declare with `extern fn` can actually be linked:
`burxt run pay.bx cside.o -lm`.

Run the test suite with `cargo test`. It is data-driven: every program in
`tests/pass/` must compile and produce exactly its recorded output, every
program in `tests/fail/` must be *rejected* with its recorded error, and every
program in `tests/panic/` must compile but die at runtime with its recorded
message. Adding a test means dropping two files in a directory.

## Editor support

<img src="assets/burxt-b-favicon-48.png" alt="" width="20" height="20" align="top"> Syntax
highlighting, live diagnostics and hover, with no `npm install`:

```sh
python3 editors/vscode/pack.py                            # no npm, no vsce
code --install-extension editors/vscode/burxt-0.1.1.vsix  # then reload the window
```

That gives VS Code syntax highlighting, **errors as you type, and hover** — with no
`npm install`. Hovering a value shows its exact type *and what that type
guarantees*:

```text
Decimal<2, RoundHalfEven>

Exact decimal, 2 decimal places. A result that needs rounding rounds half to even
(banker's rounding).
```

Syntax highlighting for VS Code (and any editor that reads TextMate grammars)
lives in [`editors/`](editors/). The grammar is checked against the compiler by a
test, so a keyword cannot exist in one and not the other.

For every other editor, **diagnostics as you type** come from `burxt lsp`, a
language server over stdio:

```lua
-- Neovim, no plugin manager needed; Helix and Zed configs are in editors/ too
vim.lsp.start({ name = "burxt-lsp", cmd = { "burxt", "lsp" } })
```

Errors carry a position, so the terminal prints the offending line with a caret
under it, and `burxt check file.bx --json` gives editors and CI the same
diagnostic with LSP-ready positions.

[`editors/README.md`](editors/README.md) records what is still missing (hover,
go-to-definition, a tree-sitter grammar for Neovim/Helix colour) and why `.bx`
files are not yet coloured on github.com.

## Design

Burxt is designed deliberately, with decisions recorded rather than improvised. `DESIGN.md` holds the design north star (identity, OOP model, SOLID stance, correctness principles, signature grammar) and the roadmap; [`docs/log/`](docs/log/) holds the milestone log, one file per stretch of versions. Milestone specs (aggregate ABI, interfaces) and a superseded/deferred-features ledger keep the reasoning trail auditable — the language stays small on purpose.

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) first — Burxt is built with a specific discipline (small verified increments, resist the kitchen sink) that keeps it coherent, and understanding it before proposing changes will make your contribution land.

## License

Licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option. Unless you explicitly state otherwise, any contribution you submit for inclusion in Burxt shall be dual-licensed as above, without any additional terms or conditions.

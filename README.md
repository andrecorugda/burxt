# Burxt

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

**Working today:** exact decimal arithmetic with explicit rounding contracts (`RoundHalfEven` / `RoundHalfUp`) and overflow-trapping, checked arithmetic; integers, booleans, and string literals; `let` / `let mut`; functions with recursion; `if` / `else if` / `else` and `while`; nominal structs with value semantics and field mutation; fixed-size arrays with always-on bounds checks; methods with value or mutating receivers; traits with static and `dyn` dispatch; a C FFI (`extern fn`); and native compilation to a standalone executable.

**Designed and committed, not yet built:** no null (absence as an explicit `Option<T>` the compiler forces you to handle); errors as values you must handle; exhaustive matching; correctness contracts (`requires` / `ensures`) checked at compile time — the verification layer that is Burxt's eventual differentiator; opt-in safe single inheritance; and the cross-compilation targets above.

`DESIGN.md` records all of it — the design north star, every milestone, and a ledger of superseded decisions and deliberately deferred features with the trigger that would earn each one a milestone. The distinction between shipped and planned is kept honest there on purpose.

## Building

Burxt's compiler (the bootstrap/stage-0 compiler) is written in Rust and emits native code via LLVM 18.

Requirements:
- Rust (via [rustup](https://rustup.rs))
- LLVM 18 development libraries. On Debian/Ubuntu: `llvm-18-dev`, `libpolly-18-dev`, `libzstd-dev`, `clang-18`. On macOS: `brew install llvm@18`.

```sh
# point the LLVM bindings at your LLVM 18 install
export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18     # or $(brew --prefix llvm@18) on macOS

cargo build
./target/debug/burxt run money.bx               # prints 59.97
```

Commands:
```
burxt build   <file.bx>     compile to a native executable
burxt run     <file.bx>     compile, then run
burxt emit-ir <file.bx>     print the generated LLVM IR
burxt layout  <file.bx>     print struct layouts (size, alignment, field offsets)
```

Run the test suite with `cargo test`. It is data-driven: every program in
`tests/pass/` must compile and produce exactly its recorded output, every
program in `tests/fail/` must be *rejected* with its recorded error, and every
program in `tests/panic/` must compile but die at runtime with its recorded
message. Adding a test means dropping two files in a directory.

## Design

Burxt is designed deliberately, with decisions recorded rather than improvised. `DESIGN.md` holds the design north star (identity, OOP model, SOLID stance, correctness principles, signature grammar) and the roadmap. Milestone specs (aggregate ABI, interfaces) and a superseded/deferred-features ledger keep the reasoning trail auditable — the language stays small on purpose.

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) first — Burxt is built with a specific discipline (small verified increments, resist the kitchen sink) that keeps it coherent, and understanding it before proposing changes will make your contribution land.

## License

Licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option. Unless you explicitly state otherwise, any contribution you submit for inclusion in Burxt shall be dual-licensed as above, without any additional terms or conditions.

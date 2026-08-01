<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/burxt-lockup-dark.png">
  <img src="assets/burxt-lockup-light.png" alt="Burxt" width="320">
</picture>

**A typed, compiled, native language where exact decimals are the default
and correctness is enforced by the compiler — not left to discipline.**

[![CI](https://github.com/andrecorugda/burxt/actions/workflows/ci.yml/badge.svg)](https://github.com/andrecorugda/burxt/actions/workflows/ci.yml)
[![Self-hosting](https://img.shields.io/badge/self--hosting-byte--identical%20fixpoint-111)](spec/M4-SELF-HOSTING.md)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-111)](#licence)

**[burxt-lang.org](https://burxt-lang.org)**

[The guide](https://burxt-lang.org/guide/) · [Install](https://burxt-lang.org/install/) · [Examples](https://burxt-lang.org/examples/) · [Design notes](DESIGN.md)

[![Open in GitHub Codespaces](https://github.com/codespaces/badge.svg)](https://codespaces.new/andrecorugda/burxt?quickstart=1)

**Try it without installing anything** — a browser, the real compiler, and the editor extension
with live diagnostics.

</div>

---

```burxt
print("Hello, world!");
```

That is a complete Burxt program. There is no entry point to declare — your top-level
statements *are* the program, and the compiler writes `main` for you.

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
- **Composition-first OOP** — small interfaces, explicit `implement Trait for Type` conformance, static dispatch by default and runtime dispatch only where you write `dynamic`.
- **Native, cross-platform by design** — one LLVM backend, many targets: desktop, mobile, and web (WebAssembly). The front end knows nothing about any platform, so reach is a configuration problem rather than a rewrite.

## Status

Burxt is early and built in small, verified increments. It is **not yet ready for production use** — it is ready to try, read and shape.

**Burxt compiles Burxt, and the two compilers agree.** The compiler is written in Burxt — lexer, parser, typechecker and an LLVM-IR backend, **10,981 lines** of it — and it compiles its own source. The compiler *it* produces emits **byte-identical** output for that same source: the fixpoint that says the two implementations agree about the whole language, rather than about the programs someone thought to test.

**The Burxt compiler compiles all 144 pass programs — 0 refused — and every one prints the same bytes as the Rust compiler's build of it** — Decimals, `match`, `return tail`, `external function`, interpolation, generics and maps included.

**And it keeps every runtime guarantee.** "Compiles every program" would read like more than it is on its own, because that measure only covers programs that *succeed*. So a second test runs the 31 programs in `tests/panic/` — a broken contract, an overflow, an index out of range, a `decreases` measure that does not decrease — through stage-1's backend and requires each one to fail. **It keeps 31 of 31**, and that is an equality rather than a floor, so losing one is a failing test.

Worth knowing how that number got written down: when the test was first added it was **8 of 21**. Stage-1 had been compiling every program correctly and silently discarding contracts, bounds checks and the termination measure — because every contract fixture in `tests/pass/` has contracts that *succeed*, and a satisfied contract produces identical output whether or not it was ever checked. The gap was shaped exactly like a directory boundary.

The Rust compiler stays as the trust anchor and as the other half of a differential test, so a change to the language has two implementations that must agree or a test fails. Details in [`spec/M4-SELF-HOSTING.md`](spec/M4-SELF-HOSTING.md).

Every push runs **74 invariants**, including that fixpoint, the differential test, 153 pass and 307 fail fixtures, and performance ratios that fail if a known quadratic returns.

**Working today:**

- **Numbers.** Exact decimals with explicit rounding contracts (`RoundHalfEven` / `RoundHalfUp`), money and percent literals (`$19.99`, `8.25%`), overflow-trapping checked arithmetic, i128 intermediates so the overflow error means the *result* does not fit.
- **The basics.** Integers, booleans, strings (length, byte access, equality, concatenation, interpolation both as a print and as a value), `let` / `let mutable`, functions, recursion, `if` / `else if` / `else`, `while`, `&&` / `||` / `!` with real short-circuiting.
- **Types.** Nominal records with value semantics, fixed-size and growable arrays with always-on bounds checks, sum types with **exhaustive `match`** (no wildcard, so a new variant breaks every incomplete match), methods with value or mutating receivers, interfaces with static dispatch by default and `dynamic` fat-pointer dispatch only where written.
- **Memory.** **Regions** as the unit of ownership: a bump allocator, release in O(1), no GC and no refcounts, and compile-time escape checking that refuses returning region storage. Single-owner regions are what make data-race freedom reachable without per-object borrow checking.
- **Guaranteed tail calls.** `return tail f(...)` is a *checked* guarantee (LLVM `musttail`), not an invisible optimization: 50 million frames in constant stack, or a compile error explaining why the guarantee cannot be given.
- **The C boundary.** `external function`, plus **exactness that survives it**: a `Decimal` crosses only through a declared marshaller (`amount: Decimal<2> as scaled`), a `Decimal` → C `double` crossing is a compile error, and an `Int` → `double` crossing is range-checked at 2^53.
- **File input.** `read_file` and `to_string`, the two things a self-hosted compiler cannot do without.
- **Contracts, checked.** `requires` / `ensures` / `decreases` on a signature, checked at runtime with no build mode that strips them, `old(...)` in an `ensures` so a conservation law is expressible (`ensures from + to == old(from + to)`), and `pure` as a compiler-checked claim that a function's answer depends on its arguments alone.
- **Self-hosting.** The whole compiler, in Burxt: a lexer, a parser building an arena AST, a typechecker that agrees with this one on every program in the suite, and an LLVM-IR backend — compiling itself to a fixpoint.

**Designed and committed, not yet built:** no null (absence as an explicit `Option<T>` the compiler forces you to handle); errors as values you must handle; *static proof* of the contracts that are checked at runtime today — the verification layer that is Burxt's eventual differentiator; algebraic effect handlers instead of coloured `async`; and the cross-compilation targets above. Single inheritance was **dropped**, not deferred: composition and interfaces do the work, and the reasoning is recorded rather than reversed quietly.

`DESIGN.md` records the design north star and a ledger of superseded decisions and deliberately deferred features, each with the trigger that would earn it a milestone. Every version's entry — what it decided and what it cost — is in [`docs/log/`](docs/log/). The distinction between shipped and planned is kept honest in both, on purpose.

## Running a Burxt program

Three ways, depending on how you like to work.

**In a terminal.** Write a file, run it — there is no project file, no manifest, no build step to configure:

```sh
$ cat > hello.bx <<'EOF'
let name: String = "world";
region r {
    print("hello, " + name + "!");
}
let price: Decimal<2> = 19.99;
print(price * 3);
EOF

$ burxt run hello.bx        # compiles to native code, then runs it
hello, world!
59.97
```

`burxt run` compiles through LLVM to a real executable and runs it — there is no interpreter and no VM, so what runs is the same machine code you would ship. Use `burxt build hello.bx -o hello` to keep the binary, and `-o` on either to say where it goes.

**In VS Code.** Install the extension (below) and open a `.bx` file: you get syntax highlighting, live diagnostics as you type, hover, and a **▶ Run button** in the editor title bar — or `Ctrl+F5`. The program runs in a terminal, and the executable goes to a temp path rather than beside your source.

**As an installed command.** `cargo install --path .` puts `burxt` on your PATH, so `burxt run x.bx` works from anywhere. A prebuilt binary that needs no Rust toolchain is [on the list](#status), not done.

What you cannot do yet: run Burxt in a browser (the WebAssembly target is designed, not built), or split a program across files (the module system is the next language milestone — today a program is one file).

## The standard library

```burxt
use "lib/string.bx";
use "lib/files.bx";
use "lib/os.bx";

region r {
    let names: [String] = file_list_directory("/etc");
    print(string_join(names, ", "));
}
```

Written in Burxt, from the same builtins any program has — nothing in it is privileged.
[`lib/`](lib/) holds strings (`string_find`, `string_split`, `string_trim`, `string_join`, `string_to_int`),
files (`file_read`, `file_append`, `file_exists`, `file_list_directory`, `file_delete`) and the machine
(`os_args`, `os_run`, `os_capture`, `os_now`).

The real reason it exists: Burxt refuses a C return whose ownership it cannot describe, so
`opendir` and `getenv` are out of reach directly, and **every program that works around that
itself is making a promise the compiler cannot check.** Here the promise is made once, in the
open, with the reasoning beside it.

## Learning it

- **[The guide](docs/guide/)** — ten pages in reading order: getting started, numbers and
  money, types, memory, contracts, the C boundary, modules, generics, absence and failure, maps
  — plus a full reference of every keyword, builtin, operator and error message.
- **[The examples](examples/)** — one program per idea, all of them compiling. Each ends
  with a *"what the compiler refuses, and why"* section quoting the real error text, because
  what a compiler declines to compile says more about it than what it accepts.
- **[The design log](docs/log/)** — every version, what it decided and what it cost.

## Installing without Rust

From a published release — Linux x86-64, the only platform built and tested here:

```sh
sh scripts/install.sh https://github.com/andrecorugda/burxt/releases/latest/download/burxt-linux-x86_64.tar.gz
```

Or build the artifact yourself:

```sh
sh scripts/release.sh          # → dist/burxt-<version>-linux-x86_64.tar.gz, and smoke-tests it
sh scripts/install.sh          # → /usr/local/bin/burxt + /usr/local/lib/burxt/
```

`PREFIX=~/.local sh scripts/install.sh` installs somewhere else.

**What you need: a C compiler.** That is the whole list. The binary **statically links LLVM**, so
there is no Rust, no cargo, no LLVM to install and no version to match — `burxt build` hands its
object file to the system linker, so `cc` has to exist. `burxt check` needs nothing at all.

**What the programs you compile need: libc.** A Burxt executable is about **16 KB** with no runtime
behind it — the allocator, the string operations and the overflow checks are emitted into every
module.

The sizes, measured rather than estimated: **48 MB** for the stripped binary, **18 MB** compressed
in the tarball. Almost all of that is LLVM, and it is a deliberate trade — one download now against
a version-matched system dependency forever, which is the commonest first-run failure for languages
that go the other way.

`scripts/release.sh` unpacks its own tarball into a scratch directory and compiles a program with
the *unpacked* binary before it reports success, so a broken artifact fails at build time rather
than on somebody's machine.

## Building the compiler

Burxt's compiler (the bootstrap/stage-0 compiler) is written in Rust and emits native code via LLVM 18. The Burxt-written compiler in `src/burxt-compiler/main.bx` is built by it — see [self-hosting](#status).

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
burxt layout  <file.bx>                  print record layouts (size, alignment, field offsets)
```

Arguments after the source file are passed to the system linker unchanged, so the
C you declare with `external function` can actually be linked:
`burxt run pay.bx cside.o -lm`.

The suite also runs **on Burxt**: [`tests/runner.bx`](tests/runner.bx) walks the same
fixtures and reports the same verdict, and a Rust test asserts the two agree — including
the case where that runner is itself compiled by the Burxt compiler.

```sh
burxt build tests/runner.bx -o /tmp/burxt-runner && /tmp/burxt-runner
running the suite with ./target/debug/burxt
ran 299, passed 299, failed 0
all green
```

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
code --install-extension editors/vscode/burxt-0.1.3.vsix  # then reload the window
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

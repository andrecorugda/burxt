# Contributing to Burxt

Thank you for your interest in Burxt. This project is built with a deliberate discipline, and contributions that understand it are far more likely to be merged. Please read this before opening a large PR.

## The philosophy (read this first)

Burxt's guiding rule is from its design north star: **pick a few signature features, resist the kitchen sink.** A language novel on every line gets admired and unused. Almost every feature request is individually reasonable and collectively a swamp. So the bar for a new feature is not "is this nice?" — it is:

> **Is there a concrete, required program that cannot be written without it?**

If a feature is merely convenient, it is deferred and written into the deferred-features ledger with the trigger that would earn it a future milestone — not added now. This is how the design phases stay honest.

Two consequences for contributors:
- **Small, focused PRs win.** A PR that adds one well-scoped capability with tests is easy to review and merge. A PR that adds five is not.
- **Design changes need discussion first.** For anything touching the type system, grammar, or object model, open an issue describing the problem before writing code. The design is recorded in `DESIGN.md`, the milestone log in `docs/log/`, and the milestone specs in `spec/`; changes to it are deliberate and get documented (superseded decisions are marked, not silently swapped).

## How the code is structured

The compiler is written in Rust with a strict front-end / back-end split:

- `src/lexer.rs` — source text → tokens
- `src/parser.rs` — tokens → AST
- `src/ast.rs` — the AST and type definitions
- `src/typeck.rs` — typechecking; **this is where the language's correctness thesis is enforced**
- `src/codegen.rs` — typed AST → LLVM IR → native object. **The only file that touches LLVM.**
- `src/main.rs` — the `burxt` CLI driver

**Keep the front end platform-independent.** Only `codegen.rs` may know about LLVM or any target detail. This separation is what makes cross-platform support a configuration problem rather than a rewrite; please preserve it.

## The working discipline

Burxt is developed in small, verified increments. Please follow the same rhythm:

1. **Never leave the tree not compiling.** Build and run the tests after each change.
2. **Tests are the product.** Because Burxt's identity is "the compiler refuses to let dangerous things happen," the *rejection* tests matter as much as the acceptance tests. A feature PR should include (a) programs that must compile and produce expected output, and (b) programs that must be *rejected* with a clear, English error message.
3. **Error messages read as advice.** A rejection should tell the user what to do, not just what went wrong. Match the existing style.
4. **Match the existing code style** and the design recorded in `DESIGN.md`.

## Getting set up

See the build instructions in [README.md](README.md). In short: Rust via rustup, LLVM 18 dev libraries, `LLVM_SYS_181_PREFIX` pointed at your LLVM 18 install, then `cargo build` and `cargo test`.

## Submitting

1. Open an issue for anything beyond a small fix, so the approach can be agreed before you invest time.
2. Keep the PR focused on one thing.
3. Include tests, including rejection tests where the change affects what the compiler accepts.
4. Make sure `cargo build` is warning-free and `cargo test` is green.

## Licensing of contributions

Burxt is dual-licensed under MIT and Apache-2.0. By submitting a contribution, you agree it will be licensed under both, without additional terms. You retain copyright to your contributions.

## Code of conduct

Participation in this project is governed by the [Code of Conduct](CODE_OF_CONDUCT.md). Please be respectful and constructive.

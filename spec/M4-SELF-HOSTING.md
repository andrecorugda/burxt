# Burxt — Self-Hosting: The Staging

> Status: **in progress.** The far-horizon roadmap describes M4 as *"a capability
> certificate, not a feature"*. This file is the plan with numbers in it, written
> after measuring the compiler rather than guessing at it.

## 0. What "self-hosted" means here, precisely

A **stage-1** compiler, written in Burxt, compiled by the Rust **stage-0** compiler,
which compiles the same Burxt source stage-0 does and produces a working program.
Verified by the standard test: compile stage-1 with stage-0, then compile stage-1
with *stage-1*, and compare the outputs. If they match, the language compiles itself.

## 1. The one architectural decision

**The stage-1 backend emits textual LLVM IR (`.ll`) and hands it to `llc`.** It does
not drive LLVM's C API, and cannot: `extern fn` returns are `Int` and `CInt` only,
because Burxt refuses to receive a pointer whose ownership it cannot describe — so
an `LLVMBuilderRef` is unreachable by construction.

This is not a workaround. Emitting text is *simpler* than driving a builder: string
formatting instead of an API, and the output is inspectable, diffable and testable
without a debugger. It also means the stage-1 backend needs nothing from the host but
a file to write.

## 2. Measured sizes

The Rust compiler is 11.5k lines. Stage-1 replaces the front end and backend, not the
language server, JSON layer or diagnostics rendering.

| Piece | Rust | Burxt estimate | Written |
|---|---|---|---|
| Lexer | 661 | 800–1,000 | **376, DONE** (v0.0.52) — came in under estimate |
| AST + parser | 1,787 | 2,000–2,600 | **633 for 3a** (v0.0.53); items remain |
| Typechecker | 3,702 | 4,500–5,500 | 385 (scale rule, symbol table) |
| Backend (IR text) | 3,924 | 2,500–3,500 | 0 |
| Driver | 230 | ~150 | 0 |
| **Total** | | **≈10,000–12,500** | ~680 of real front-end work |

Burxt runs 1.2–1.5× the line count of equivalent Rust: no generics, no closures, no
`?`, no `match` as an expression, no maps. That is priced in above.

## 3. Phases, each one shippable

1. **Driver primitives** — `arg(n)`, `arg_count()`, `write_file`, and a region large
   enough to hold a whole compile. **DONE (v0.0.51).**
2. **A full lexer in Burxt.** **DONE (v0.0.52)** — `examples/stage1_lexer.bx`, 376
   lines: every punctuation form, a 39-entry keyword table with type names
   distinguished, strings with escapes, interpolation *detected* (splitting the pieces
   is the parser's job), comments, and exact money and percent literals. It lexes its
   own source and every program in the pass suite, checked by a test as a
   disagreement-finder between stage-0 and stage-1.
3. **A full parser in Burxt**, producing an arena AST — children by index, which
   v0.0.22 already proved needs no recursive types.
   - **3a DONE (v0.0.53):** types, the full expression ladder, struct and array
     literals, and every statement form including `match` with bindings. Child lists
     live contiguously in a side array, so nothing needs back-patching. Parses every
     source in the repository, including its own.
   - **3b:** items — `fn`, `struct`, `enum`, `trait`, `impl`, `extern` — with their
     markers and contract clauses. The driver currently steps over them.
4. **A full typechecker in Burxt.** The big one, and the one where the language will
   hurt most: 4,500 lines of rules with linear-search symbol tables and one source
   file.
5. **An IR-text backend in Burxt.**
6. **Bootstrap and fixpoint.** stage-0 builds stage-1; stage-1 builds stage-1;
   compare.

**The public milestone (Andre's decision, v0.0.46) is the end of phase 4**, not phase
6: a Burxt-written lexer, parser and typechecker running under the Rust compiler.

## 4. Risks, named

- **Determinism.** The fixpoint test is meaningless if stage-0's output varies between
  runs. **Checked in v0.0.50:** three compiles of the same file produce byte-identical
  IR, and no HashMap is iterated to produce output. This is the risk that quietly
  kills bootstraps, and it is absent.
- **Region size.** One region holds an arena, a symbol table and every interned name
  for a whole compile. Raised from 64 MB to 1 GB in v0.0.51 — lazily mapped, so the
  cost is virtual.
- **O(n²) lookups.** Symbol tables are linear searches. Fine at bootstrap scale;
  a map earns its place when a compile is measurably slow, not before.
- **One file.** No module system, so stage-1 will be one large file. Tolerable, ugly,
  and recorded rather than fixed.
- **Compile time and stack.** Stage-0 walks expression trees recursively on a 512 MB
  stack. A 10k-line stage-1 is far below the measured ceiling (~30,000 operator
  terms), so this is not expected to bite — but it is the reason the ceiling was
  measured.

## 5. What this must NOT do

- **NO abandoning stage-0.** It stays as the trust anchor and the cross-check. The
  Thompson "trusting trust" concern is a real one, and a second implementation that
  can compile the first is the answer to it.
- **NO features invented for the compiler's convenience** that a user program would
  not want. Every gap self-hosting finds gets the same test: would anyone else need
  this? `break` passed that test (v0.0.50). A `HashMap` builtin would not.
- **NO half-migration.** Each phase produces something that runs and is compared
  against stage-0's answer for the same input, rather than a partial rewrite that
  compiles nothing.

## 6. Why this method, not just this milestone

Every self-hosted piece so far has found a real defect in the Rust compiler:

- the lexer rewrite (v0.0.21) corrected three wrong assumptions about what the
  language needed,
- the parser rewrite (v0.0.22) disproved a claim that it was blocked on the memory
  model,
- the symbol table (v0.0.47) fired a deferred trigger and forced `allocates` onto
  methods,
- designing the checker's error type (v0.0.48) found a **use-after-free** the escape
  checker had accepted since regions shipped.

Four for four. Self-hosting is the best test suite this project has, and that is the
argument for doing it now rather than at the end.

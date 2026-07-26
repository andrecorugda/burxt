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
| AST + parser | 1,787 | 2,000–2,600 | **~930, DONE** (v0.0.53–54) — under estimate |
| Typechecker | 3,702 | 4,500–5,500 | **~2,190**, 4b complete (v0.0.64) |
| Backend (IR text) | 3,924 | 2,500–3,500 | **~450, slice 1 running** (v0.0.65) |
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
   - **3b DONE (v0.0.54):** items — `fn`, `pure fn`, methods, `struct`, `enum` with
     payloads, `trait`, `impl`, `extern fn` — with `allocates`, `requires`, `ensures`,
     `decreases` and `as scaled`. **stage-1 parses its own source**: 6,610 nodes, no
     errors. Still open: splitting interpolation fragments into pieces, and a trait
     signature's parameters beyond the receiver.
4. **A full typechecker in Burxt.** The big one, and the one where the language will
   hurt most: rules with linear-search symbol tables and one source file.
   - **4a DONE (v0.0.57):** declarations collected in a first pass, expressions typed
     with the expected type carried inward (which is how a Decimal product knows
     whether a contract was supplied), statements checked including `let`, assignment,
     mutability, `return` against the signature, and conditions. **stage-1 typechecks
     its own source with zero complaints.** 22 of 87 pass programs still draw a
     complaint stage-0 does not — that is the progress bar for 4b.
   - **4b, part DONE (v0.0.58):** field access, struct literals, the builtins, and
     enum constructors — `Cell.Number(3)` is indistinguishable from a field access
     until you check whether the base names a type. False positives across the pass
     suite: **24 of 88**, from 22 before (adding checks made it worse before better,
     which is the honest direction). Remaining: methods on values, match bindings
     against variant payloads, indexing element types, and the region and purity
     rules.
   - **4b, part DONE (v0.0.59):** match arms bind their variant's payload types (and
     an arm that binds the wrong number of names is refused), indexing answers the
     element type, a String reports that it is read with `byte_at`, and a method call
     on a value is resolved against a method table. **False positives: 19 of 88**,
     down from 24 while *adding* rules.

     This is also where self-hosting found its sixth defect — in stage-1 itself, and
     a structural one: **a child list built by pushing into the shared `kids` array is
     not contiguous**, because parsing element two pushes element two's own children
     first. v0.0.57 fixed three symptoms of this by moving nested lists to a second
     array; the disease was the pattern. Every list now goes through a scratch
     **stack**: children are pushed there, and `commit` moves them into `kids` in one
     block once they all exist. A stack is the right shape because an inner list is
     committed and popped before an outer list continues, at any nesting depth.

     The reason it hid so long is worth recording: reading a garbage child index
     yields a node of some other kind, and a checker that dispatches on kind then
     *checks nothing* — silence that looks exactly like agreement. Match arms were the
     first rule whose failure could not be silent, because a binding that never
     entered the symbol table becomes "unknown name".
   - **4b DONE for the type rules (v0.0.60):** **0 false positives across all 88 pass
     programs**, from 19, and stage-1 typechecks its own 2,411 lines with none. What
     closed the gap, in the order the measurement pointed at:
     - a decimal literal takes its context's scale when that loses nothing (`$5` in a
       `Decimal<2>` is `5.00`), and refuses when it would drop a digit;
     - `Decimal / Decimal` and `Decimal / Int` need a rounding contract — unlike `*`,
       even when the scales match, because a quotient falls between representable
       values;
     - `*` on two identical Decimal types lands on their own contract, so the context
       does not have to repeat it;
     - an operand of `*` keeps its own scale rather than being forced to the result's,
       which is how `price * 8.25%` survives (stage-0 reaches the same answer by
       re-checking both operands with no expected type);
     - `CInt`/`CDouble` in an `extern` signature are recorded as the `Int` a Burxt
       caller passes, which is why `strcmp(a, b) < 0` compares two Ints;
     - methods declared inside `impl` blocks are collected, and **method bodies are
       checked at all** — with `self` bound from the clause node — which they were not
       before;
     - a method call answers its declared return type, and one shared routine checks
       arity and argument types for functions and methods alike;
     - `dyn Trait` accepts a type that implements it (`fits`, kept separate from
       `ty_same` because equality must stay equality), and a method on a `dyn` value is
       checked against the trait's signature rather than any concrete type.

     Measured the other way too, which is the honest half: stage-1 rejects **67 of the
     190 fail programs** on its own. The rest are the rules it does not yet mention —
     regions and escapes, purity, exhaustiveness, arrays, the reserved names, contract
     clauses, unreachable code. Both numbers are now machine-checked: the pass suite is
     swept in full, and the fail count is a floor that may only rise.

   - **4b DONE — regions, purity and escapes (v0.0.61):** the rules that make Burxt
     Burxt rather than a typed calculator. Stage-1 now rejects **94 of the 190 fail
     programs**, from 67, with **0 false positives** still:
     - a region opens and closes around a block, regions do not nest, and
       `has_region()` answers M1's one question — is there anywhere to allocate;
     - `allocates` on the signature means the caller's region is in effect for the
       whole body, so a call to an `allocates` function or method needs a region at the
       call site;
     - `to_string`, `substring`, `read_file`, joining two Strings and an interpolated
       String used as a VALUE all need one — but `print("a {b}")` does not, because it
       writes its pieces in order and joins nothing, and `to_string` of a Bool does
       not, because it renders to one of two literals;
     - a `pure` function may not print, read or write a file, call a function that is
       not `pure`, or call a method at all — `pure` being refused on methods is what
       makes the last one absolute;
     - the escape rule, which is M1 in one condition: a built value may leave a
       function only when it was built in the CALLER's region, meaning `allocates` with
       no region of the function's own open around the return;
     - the names the language owns (`print`, `to_string`, `old`, `main`, the int
       division builtins, …) may not be declared by a program.

     Two mechanisms were needed and both are worth naming. `expr_allocates` walks
     aggregates — the v0.0.48 lesson, since a struct holding a built String is itself
     built. And a **type cache** records every expression's type as it is decided,
     because the escape check asks about expressions the checker has already seen
     (did that `+` join two Strings?) and re-walking them would report every complaint
     a second time.

   - **4b DONE — the rest of the rules (v0.0.62):** **125 of the 190 fail programs**
     rejected, from 94, with **0 false positives** across all 88 pass programs and its
     own 3,088 lines. Three groups:
     - **the builtins**, table-driven: how many arguments each takes, and what it will
       accept — `len` of a non-array, `byte_at` of a non-String, `to_string` of a String
       or an aggregate, integer division of non-Ints, a path that is not a String,
       `push`/`truncate` on something that is not growable **or not `let mut`**, and a
       pushed value against the array's element type. Arity is reported first and alone,
       because the type errors that follow a miscount are noise;
     - **control flow**: `break` and `continue` with no loop to leave, `return` with no
       function to return from, a body that can reach its closing brace without
       returning, and code after a statement that always leaves the block. All four
       come from one question — *does this statement always exit?* — answered for `if`
       only when both sides do, for `match` only when every arm does, and never for
       `while`, since a loop may run zero times and claiming otherwise would be a guess
       about the condition. `else if` puts a statement where a block usually goes, so a
       branch is asked the same question either way;
     - **the aggregates**: a struct literal must set every field, an enum constructor's
       payload is checked against the variant it names (a constructor is the mirror of a
       match arm, and the two read alike), `match` needs an enum and needs an arm for
       every variant with no arm twice — there is no `_` to hide behind — and an enum
       may not be empty or declare a variant twice. Plus `print` of a struct, an enum,
       a `dyn` or an array, which has no rendering the language could choose.

   - **4b DONE — contracts and traits (v0.0.63):** **140 of the 190 fail programs**
     rejected, from 125, still **0 false positives**.
     - **the clauses**: `requires`, `ensures` and `decreases` are checked at all, under
       the `pure` rule A5 gives the reason for — a contract that can change the program
       is not a check. A clause must be a Bool; a measure must be an `Int`; `result` is
       bound inside `ensures` and nowhere else; `old(...)` outside an `ensures` is
       refused, `old(result)` is refused as a contradiction, and `old` of an aggregate
       is refused naming the copy that is not built; `ensures` on a function returning
       an aggregate is refused for the reason A5 §2 states; one `decreases` per
       function, none on a method, and none on a function that never calls itself.
     - **the impls**: every signature the trait declares, no extras, and the same shape
       for each — receiver form (`self` and `mut self` are different promises),
       parameter count, parameter types, return type. A trait is a promise a `dyn` value
       makes on the implementor's behalf, so a mismatch here is a promise nobody keeps.

     Two mechanisms worth naming. Binding `result` needs a *span whose bytes spell it*,
     because a symbol is a span into the source and the program never wrote the word
     where the clause is — so `find_text` finds any occurrence, and if there is none,
     nothing can refer to it. And "does this function call itself", which `decreases`
     needs, is answered by scanning the body's tokens between its braces: exact, because
     Burxt has no first-class functions, so a name followed by `(` is a call to the one
     function that has it.

   - **4b DONE — arrays, layout and `tail` (v0.0.64):** **154 of the 190 fail programs**
     rejected, from 140, still **0 false positives**. Arrays: a literal is checked
     element by element against the declared element type, a fixed array's length is
     part of its type so a literal of the wrong length is refused, a literal index past
     the end is a COMPILE error rather than an exit 70 later, arrays do not nest, an
     array of no elements is refused, and building a growable array needs a region.
     Layout: a struct cannot contain itself and an enum cannot carry itself, because the
     size would have to exceed itself; an aggregate payload is refused with the layout
     rule that is not written. And `tail`, whose five conditions are each a compile
     error rather than a quiet fallback to an ordinary call: it names a call, not inside
     a region (the jump would pass the region's close), not returning an aggregate (that
     travels through the frame being replaced), not into C (which has its own calling
     convention), and matching the enclosing signature exactly — parameter count,
     parameter types, return type.

     One correction worth recording: I first wrote the rule that a growable array in a
     struct is refused. The pass suite answered immediately — stage-1's own `Unit` is
     made of them. The real rule is that such a struct must be BUILT in a region, which
     is a statement about the literal, not the declaration.

5. **An IR-text backend in Burxt.** **Slice 1 SHIPPED (v0.0.65):** a program compiled
   by the compiler written in Burxt runs, and prints exactly what stage-0's build of the
   same source prints. That is checked by a test — `programs_compiled_by_the_burxt_
   backend_run_and_agree_with_stage_0` — which takes stage-1's `.ll` through `llc` and
   the system linker, runs the result, and compares it with stage-0's answer.

   What slice 1 emits: Ints, Bools, String literals, `+ - *` **through the overflow
   helpers** (an arithmetic result that silently wraps is the same class of wrong as a
   rounded cent, so the helpers are written into every module), comparisons, `&&`/`||`,
   `if`/`else`, `while` with `break` and `continue`, functions, calls, recursion, and
   `print` of each covered type. Locals are allocas named by their slot index rather
   than their source name, so two blocks that both say `x` cannot collide.

   Decisions worth recording:
   - **Every string byte is emitted as a hex escape** (`c"\68\69\00"`). LLVM accepts
     that, and it means no byte needs a character to stand for it — which matters in a
     language whose own `to_string` answers with digits, not letters.
   - **Anything not covered is refused by name**, never emitted wrongly: interpolation
     says it desugars to `to_string` and `+` and that neither is emitted in this slice.
     A backend that half-emits is worse than one that says what it cannot do.
   - The emitter answers **operands** — `%t7` or `42`, the caller cannot tell which —
     which is what keeps it free of a value type.

   Two defects the running program found, which reading the IR had not: comparison
   opcodes were mapped to invented token numbers rather than the lexer's (`<` is 35, not
   13), so `i < 3` emitted `icmp eq`; and a String token's span includes its quotes, so
   `print("done")` printed `"done"`. Both were found by diffing against stage-0's output
   — the only test that could have found them.

   Still to emit: Decimals and their rounding, regions and the allocator, aggregates,
   `match`, methods, `tail` with `musttail`, contracts, and the FFI boundary.
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

- in v0.0.59 the first rule with an observable failure — match bindings — exposed a
  **non-contiguous child list** in stage-1's own arena, a defect whose only symptom
  until then was a checker quietly skipping statements,

- and in v0.0.56 the front-end cross-check caught **stage-0 and stage-1 disagreeing
  about the language itself**, one version after a change to it: stage-1 still treated
  four contextual marker words as reserved, so it rejected a program stage-0 had just
  started accepting.

Six for six. Self-hosting is the best test suite this project has, and the second
implementation is a **differential test** as well as a certificate: from here on, a
change to the language has two places that must agree, and disagreement arrives as a
failing test rather than a bug report. That is the argument for doing it now rather
than at the end.

# The front end, in Burxt

*Milestone log, v0.0.51 – v0.0.58. The design these versions serve is in [DESIGN.md](../../DESIGN.md); the whole log is indexed [here](README.md).*

The driver primitives, then a lexer, a parser and a typechecker written in Burxt — each compared against the Rust compiler's answer for the same input, which is how the two implementations caught each other disagreeing about the language.

### v0.0.51: the primitives that make a program a tool

Phase 1 of `spec/M4-SELF-HOSTING.md`, which is now the plan of record with measured
numbers in it rather than an intention.

**`arg_count()` and `arg(n)`.** A compiler has to know which file it was asked to
compile, and the C runtime only offers that to `main` — so `main` now takes `argc` and
`argv` and stashes them where any function can read them. `arg(n)` is bounds-checked
like everything else, and needs **no region**: the runtime's argument strings outlive
the program, so it borrows rather than copies. That is the first String-producing
builtin that does not allocate, and the reason is worth stating rather than looking
like an oversight.

**`write_file(path, contents)`** returns the number of bytes written, so a caller can
check rather than hope. Refused inside a `pure` function, for the reason every effect
is: a function whose result depends only on its arguments does not leave marks.

**A region a compiler can live in.** The bump allocator's chunk went from 64 MB to
1 GB. Stage-1 holds an arena of AST nodes, a symbol table and every interned name for
one whole compile inside a single region, and 64 MB would not have survived it. The
cost is **virtual, not resident** — `malloc` of that size hands back lazily mapped
pages, so a program that touches a kilobyte pays for a kilobyte. Exhaustion is still a
named error rather than an overrun.

**And the plan itself is now in the repository**, with the sizes measured from the
Rust compiler (11.5k lines; stage-1 needs ~10–12.5k of Burxt), the phases, the public
milestone at the end of phase 4, and the risks named — including the one that quietly
kills bootstraps, which v0.0.50 verified is absent: three compiles of the same file
produce byte-identical IR, and no HashMap is iterated to produce output.

The spec also records the decision that makes the backend feasible at all: **stage-1
emits textual LLVM IR.** It cannot drive LLVM's C API, because `extern fn` returns are
`Int`/`CInt` only — Burxt refuses to receive a pointer whose ownership it cannot
describe, so an `LLVMBuilderRef` is unreachable *by construction*. Emitting text is
simpler anyway: string formatting instead of a builder, and output you can diff.

### v0.0.52: the stage-1 lexer, and it lexes itself

M4 phase 2. `examples/stage1_lexer.bx` is 376 lines of Burxt and is **not** a
demonstration: every punctuation form including the eight two-character ones, a
39-entry keyword table with type names distinguished from identifiers, string literals
with escapes and interpolation detection, comments, and exact money and percent
literals.

```text
lexed examples/tour.bx: 393 tokens, 39 keywords known
  decimal $19.99 -> unscaled 1999 scale 2
```

`$19.99` becomes the unscaled integer **1999 with scale 2**, accumulated digit by
digit — the thesis holding inside the self-hosted lexer, not just in the compiler that
compiled it. A percent literal comes out two places finer, exactly as the Rust lexer
makes it.

**It lexes its own source**: 3,131 tokens, zero errors. And a new test makes that a
standing guarantee rather than an anecdote — the Burxt lexer is run over **every
Burxt source in the repository**, including itself and all 81 programs in the pass
suite. Those files already compile, so the Rust lexer accepts them by definition;
any byte the Burxt lexer refuses is a **disagreement between two implementations**,
and one of them would be wrong. That is the first cross-check between stage-0 and
stage-1, and the shape every later phase will reuse.

**What the language made awkward, honestly.** Token kinds are `Int` codes rather than
an enum, because a 60-variant enum would force a 60-arm `match` at every use and the
payloads differ per kind. That is a real cost of exhaustive matching without a
wildcard — the rule that has caught genuine bugs elsewhere is a nuisance here. Kept,
because the alternative is `_`, which v0.0.20 refused on purpose.

**And a small thing the compiler got right unprompted:** three scanners had to be
declared `allocates`, because building `"error: byte " + to_string(one)` allocates —
and the compiler said so, naming the fix, in a file it had never seen before.

### v0.0.53: the stage-1 parser — types, expressions, statements

M4 phase 3a. `examples/stage1_lexer.bx` became `src/burxt-compiler/stage1.bx`, because it is no
longer a lexer: it is the stage-1 compiler, growing a phase at a time in one file, which
is the shape the plan predicted while Burxt has no modules.

**1,009 lines of Burxt** now — 376 of lexer and 633 of parser: every type form (including `Decimal<S, R>`, slices, fixed
arrays and `dyn`), the full expression precedence ladder with postfix chains, struct and
array literals, and every statement — `let`, assignment, `print`, `return`, `return
tail`, `if`/`else if`, `while`, `break`, `continue`, `region`, `match` with payload
bindings, and expression statements.

**It parses every Burxt source in the repository, including its own**, and the
cross-check test now covers both halves: any construct the Burxt parser refuses is a
disagreement with the Rust parser, and one of them is wrong.

**The arena design changed, and for a better reason than the one that forced it.**
Child lists — a call's arguments, a block's statements, a match's arms — live in a
side array of indices, with a node holding `(start, count)`. That began as a workaround:
Burxt has no `xs[i].field = v`, and cannot write to a growable array element through a
field, so the obvious linked-cell approach could not back-patch. But children pushed
into a side array land **contiguously** even though their subtrees interleave in the
node array, so a list is two integers instead of a chain — which is what production
compilers do anyway. **The language's limitation pushed the design somewhere better.**

**Three gaps found, each recorded with its trigger:**

- **`xs[i].field = v`** — assignment through an index and then a field. Utterly
  ordinary (`table.rows[i].count = 5`), so it earns its place; deferred to keep this
  phase shippable.
- **Writing to a growable array element through a field** — `self.nodes[i] = value`.
  Reading works, `push` works, writing does not.
- **The highlighter disagreed with the compiler about `\}`.** The compiler accepts it
  as an escape; the TextMate grammar's escape list did not include it, so valid code
  was flagged invalid. Fixed. That is the second time writing Burxt found a drift the
  keyword test could not see, because it checks that keywords *exist*, not that escape
  rules match.

**And a bug in my own Burxt code worth keeping.** The driver steps over items it does
not parse yet by matching braces, and treated any semicolon before the first brace as
the end of a bodyless `extern` declaration. `fn f(xs: [Int; 3])` contains a semicolon —
inside the array type — so the skip stopped mid-signature. Fixed by counting
parentheses and brackets too. A heuristic that had to meet real syntax to fail.

Items — `fn`, `struct`, `enum`, `trait`, `impl`, `extern` — are phase 3b. The driver
steps over them rather than pretending to read them.

### v0.0.54: stage-1 parses items — and parses itself

M4 phase 3b. `fn`, `pure fn`, methods with `mut self` receivers, `struct`, `enum` with
payloads, `trait` signatures, `impl Trait for Type`, `extern fn` — with the markers and
contract clauses that make a Burxt signature say what it promises: `allocates`,
`requires`, `ensures`, `decreases`, and `as scaled` on a parameter.

**The number that matters:**

```text
parsed 55 items and statements into 6610 nodes, 2263 child slots
  parse errors: 0            <- stage1.bx, parsing its own 1,300 lines
```

Every function, every method, every struct, every contract clause in the stage-1
compiler, read by the stage-1 compiler. The front end is now **self-parsing**, and the
cross-check test holds it to that over every source in the repository.

**The language caught me using its own keyword.** `let mut allocates: Int = 0;` — refused,
because `allocates` became a keyword in v0.0.38 and Burxt does not let a name shadow one.
The variable is now `builds_in_caller`, and the refusal was correct: a local called
`allocates` inside the parser that *handles* `allocates` is exactly the confusion the rule
exists to prevent.

**Markers ride as bits, and that is a deliberate arena decision.** `pure`, `allocates` and
a mutating receiver are three flags in one integer field rather than three fields, because
a node is a fixed-size struct in an array and every field is paid for by *every* node.
The same reasoning that makes real compilers pack their AST.

**What is left of the front end:** interpolation fragments are detected but not split into
pieces, and the parser records enough to rebuild a signature but not the receiver's
parameter list on a trait signature. Both are named in the spec rather than left to be
discovered.

Next is phase 4, the typechecker, which the plan calls the hardest and the largest — and
which is where the public milestone sits.

### v0.0.55: the marker words become contextual

```text
let mut allocates: Int = 0;          // legal now — an ordinary name
fn label(n: Int) -> String allocates // still the marker, in the one place it means one
```

**Prompted by a question from Andre**, after v0.0.54 hit the collision: PHP's `$var`
makes reserved-word conflicts impossible, so why doesn't everyone do that?

The answer is where the cost lands. A sigil taxes **every variable reference** —
millions across a codebase — to refund a problem that happens a dozen times in a
language's life, and it does not even remove the reserved list (PHP still forbids
`class class`, and `$this` is reserved). Perl's sigils at least encode *type*;
PHP inherited them and they encode nothing. The interpolation benefit that makes
sigils worth it in shell, Burxt already gets from a delimiter — `"total {amount}"`
with `\{` for a literal — which costs something only inside strings.

The languages that took the problem seriously solved it precisely: **contextual
keywords** (C#'s `async`, `await`, `yield`, `value` are all legal identifiers) and
**raw identifiers** (Rust `r#type`, Swift backticks). Both pay only at the collision.

**And Burxt has the problem worse than most**, because its philosophy makes it worse:
every guarantee is a declared word, so the list grows with every feature — `pure`,
`allocates`, `tail`, `requires`, `ensures`, `decreases`, and more to come.

So `allocates`, `requires`, `ensures` and `decreases` left the keyword table. Each
appears in exactly **one** position — after a return type, or between a signature and
a body — where nothing else can appear, so the parser recognises them by place rather
than by reservation. Everywhere else they are names.

**There was already a precedent in the language:** `scaled` in `as scaled` was
contextual from the day it shipped (v0.0.30) and never reserved. This makes the rest
consistent with it.

**Strictly loosening, which is why it is safe.** Programs that were errors become
legal; no valid program changes meaning. That is what the v0.0.17 syntax-change law
requires, and it is the opposite direction from the change that law was written for.

**What stays reserved, and why:** `pure`, `tail`, `let`, `if`, `break` and the rest can
begin a statement or an expression, where an identifier can also begin one. Recognising
those by position would be genuine ambiguity rather than free precision. The line is not
"which words are keywords" but "which words have exactly one possible position".

### v0.0.56: stage-1 follows stage-0, and the cross-check proved its worth

A correction, and the best evidence yet for building the second implementation early.

v0.0.55 made four marker words contextual in **stage-0** and shipped with a failing
test — I chained the commands and committed before reading the result, which is the
same mistake as the `grep -c` one in v0.0.44: **not looking at the answer.**

What failed was exactly the right thing. The front-end cross-check compiles the Burxt
lexer and parser and runs them over every source in the repository, and it reported:

```text
tests/pass/contextual_markers.bx: the Burxt PARSER reported an error the Rust parser
did not
```

Stage-1's own keyword table still held `allocates`, `requires`, `ensures` and
`decreases`, so `let mut allocates: Int = 0;` — a program stage-0 had just started
accepting — was a syntax error to stage-1. **Two implementations of the same language
disagreeing, caught within a minute, by a test written two versions earlier for
exactly this.**

Stage-1 now recognises the four by position too, comparing the token's span against
the word without allocating — the same trick its keyword lookup uses.

**The lesson is about method, not about markers.** A second implementation is not only
the M4 certificate; it is a differential test. A change to the language now has two
places that must agree, and the disagreement surfaces as a failing test rather than as
a bug report six months later. That is worth more than the milestone.

### v0.0.57: `truncate`, and stage-1 typechecks itself

M4 phase 4a. The stage-1 compiler is 1,894 lines of Burxt and now **typechecks**: it
collects every declaration, walks every expression and statement, and refuses what
stage-0 refuses.

```text
error: `*` on Decimal<2> and Decimal<4> needs a rounding contract: the exact product
       has 6 decimal places
error: cannot combine Decimal<2> and Decimal<4>: addition and subtraction need the
       same type, scale included
error: `/` on two Ints would have to round, and one operator cannot say which way
error: unknown name (at `nobody_declared_this`)
```

**The thesis, enforced by a Burxt program, over a real AST.** And the strongest single
case: `stage1.bx` typechecks **its own 1,894 lines with zero complaints.** A test holds
it to both directions — silent on what stage-0 accepts, and catching all seven mistakes
in a program written to break every rule this phase implements, inventing none.

Honest measure of what remains: **22 of 87 pass programs still draw a complaint stage-0
does not**, all from what phase 4b covers — field access, struct literals, methods,
match bindings, builtins. That number is the progress bar.

**`truncate(xs, n)` had to be added to the language**, and it is the clearest "earned
its place" case yet. Leaving a block must drop the bindings it made, and Burxt had no
way to make a growable array shorter: `push` and reading worked, shrinking did not. So a
scope could only ever grow, and a function's parameters stayed visible forever — which
showed up immediately as a **false "already declared"** on a top-level name that matched
a parameter. The buffer is kept, so a scope that pushes and truncates reuses the same
memory instead of growing; a length above the current one is a named runtime error,
because exposing elements that were never written is exactly the silent wrongness this
language refuses.

**And a bug in my own Burxt worth keeping**, because it broke the design's premise. The
arena's whole idea is that a child list is *contiguous*, so a node stores `(start,
count)`. I then pushed a match arm's **bindings** into the same array as the **arms** —
interleaving two lists, so reading `count` arms from `start` walked past the end. It
read index 95 of 91.

The cross-check test found it on `tests/pass/enum_match_flow.bx` within seconds. Nested
lists now have their own array, and the comment above it says why it is correctness
rather than tidiness. Three places had the same shape: match bindings, enum payload
types, trait signature parameters.

### v0.0.58: stage-1 learns fields, struct literals, builtins and constructors

M4 phase 4b, partly. The stage-1 typechecker now types **field access** (walking the
struct's declaration to find the field's type), **struct literals** (every field
checked against what was declared, and a name that is not a field is refused), the
**builtins** (`len`, `to_string`, `substring`, `push`, `truncate`, `div_floor`, `arg`,
`read_file`, `write_file` and the rest — they are not in the function table because
nothing declares them, so the checker has to know them the way the compiler does), and
**enum constructors**.

**That last one is the interesting case.** `Cell.Number(3)` parses as a field access —
or a method call — on `Cell`, and the checker typed `Cell` as a variable and reported
an unknown name. An enum constructor is *syntactically indistinguishable* from a field
access until you look at whether the base names a type. The fix is three lines and one
check, and it is the kind of thing only real programs reveal: nothing in the hand-made
tests looked like this.

**The measure moved the honest way.** Adding checks made the count *worse* first — 22
false positives became 26, because a checker that types more things has more chances to
be wrong. Handling constructors brought it to **24 of 88**. Reporting the number that
went up is the point: the alternative is a checker that stays silent and scores well.

`stage1.bx` still typechecks **its own 2,000 lines with zero complaints**, which is the
case that matters most.

What remains for 4b: methods (`x.m()` where the base is a value), match bindings against
variant payloads, indexing element types, and the region and purity rules — which
stage-0 enforces and stage-1 does not yet mention.

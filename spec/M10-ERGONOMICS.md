# Burxt — the ergonomics that make it usable (M10)

> Status: **slices 1 and 2 DONE (v0.0.91–v0.0.92).** `let x = e;` and `for x in xs` work in
> both compilers, the compiler's own source uses both, and the fixpoint holds. The rounding
> rule got more correct on the way: a contract is now required exactly where a value narrows.
> Slice 3 (generics) is next.
>
> The bar, in Andre's words: **as easy as Python but typed, as friendly as PHP but never
> compromised.** If a construct is harder to write than its Python equivalent and the extra
> ceremony buys no correctness, the ceremony is the bug — and friendliness may never cost
> exactness.
>
> Original status: **slice 1 implementing.** The language is correct and it is
> self-hosting; what it is not yet is *pleasant*. Every one of these is a thing a reader of
> Burxt already expects to exist, and every one of them is sugar over something the language
> already means — never a second way to mean it.

## 0. Why this is a milestone and not a tidy-up

DESIGN.md states the tension outright: Burxt wants many guarantees *and* Python-like ease, and
"the compiler should be strict silently, not loud." Everything shipped so far has been the
strict half. A language that is correct and unpleasant does not get used, and unused is the one
failure mode no amount of correctness fixes.

The slices, in the order they earn their place:

| Slice | What it is | State |
|---|---|---|
| 1 | `let x = 0;` — a binding takes its type from its initializer | **DONE** (v0.0.91) |
| 2 | `for x in xs { }` — the loop everyone writes, without the index | **DONE** (v0.0.92) |
| 3 | Generics ([M7](M7-GENERICS.md)) | **next** |
| 4 | `Option<T>` / `Result<T, E>` and `?` ([M8](M8-ERRORS.md)) | specified |

## 1. Slice 1 — local type inference on `let`

### Decision 1 — `let x = e;` takes its type from `e`, and nothing else infers

```text
let count = 0;                       // Int
let name = "burxt";                  // String
let price = $19.99;                  // Decimal<2>
let rate = 8.25%;                    // Decimal<4>
let origin = Point { x: 0, y: 0 };   // Point
let state = Status.Paid;             // Status
let doubled = double(21);            // Int, from the signature
let greeting = "hi, " + name;        // String, built in the region
```

Arrays are the exception, and Decision 2 says why.

**Signatures stay explicit.** Parameters, return types, record fields, `allocates`, `pure` and
every contract are written down. Inference is local to one statement, so a reader never has to
look past the line in front of them to know what a binding holds — and the places a *reader of
someone else's code* needs types most are exactly the places that keep them.

This was flagged in `spec/README.md` at v0.0.18 as deserving its own decision rather than being
smuggled in with `$19.99`. This is that decision.

### Decision 2 — the annotation stays legal, and an array always names its type

```text
let xs: [Int; 3] = [1, 2, 3];        // fixed
let mutable lines: [String] = [];    // growable
```

```text
error: an array literal does not say whether the array is fixed or growable, so an
       array binding names its type: `let xs: [Int; 3] = [1, 2, 3];` for a fixed
       one, or `let mutable xs: [Int] = [];` for one that grows.
```

This is the one place local inference does not serve, and the reason is not the element
type — `[1, 2, 3]` obviously holds Ints. It is that **fixed and growable are different
types with different storage, different rules and different costs**, and a list of values
does not say which one was meant. Guessing would mean picking the cheap one and making
`push` fail later, or picking the flexible one and putting every array in a region.

Stage-1 makes the same refusal for an additional reason worth recording: its `Ty` names an
array's element type by *the node of a type annotation*, so with no annotation there is no
element type to point at. Two implementations agreeing that a rule is right for different
reasons is a good sign about the rule.

An annotation is never wrong and never redundant-by-rule. Write one wherever it helps.

### Decision 3 — inference removes typing, not checking

Every rule that applied to an annotated binding applies unchanged:

```text
let subtotal = $122.97;              // Decimal<2>
let rate = 8.25%;                    // Decimal<4>
let exact = subtotal * rate;         // Decimal<6> — exact, no rounding
let total = subtotal + exact;        // still an error: scales must match
```

**And inference can never introduce rounding.** A rounding contract only exists if someone
wrote it, so `let tax = subtotal * rate;` infers the exact `Decimal<6>` and the compiler still
demands a decision before that becomes money:

```text
let tax: Decimal<2, RoundHalfEven> = subtotal * rate;   // the rounding, still written down
```

That is the property that makes inference safe *in this language specifically*: the thing worth
being loud about is attached to the annotation, so dropping the annotation cannot drop it.

### Decision 4 — what cannot be inferred is refused with the reason, never guessed

Burxt has no literal whose type is ambiguous — `0` is an `Int`, `19.99` is a `Decimal<2>`,
`8.25%` is a `Decimal<4>` — so there is no defaulting rule to learn and no `0i64` to write. The
one construct with a real choice behind it is the array literal, and it is an error naming its
fix rather than a guess (Decision 2).

### Decision 5 — a rounding contract is required where rounding happens, and not before

Inference forced this out into the open, so it is recorded here rather than only in the log.
`Decimal<2> * Decimal<4>` has an exact product with **six** decimal places. Until v0.0.91 the
compiler demanded a rounding contract for that multiplication *always* — including when the
target was `Decimal<6>`, where nothing rounds. It had to, because a bare `a * b` had nowhere
for a contract to live and the rule could not tell "exact" from "narrowed".

Now the rule is the true one:

```text
let exact = price * rate;                              // Decimal<6> — exact, no contract
let exact6: Decimal<6> = price * rate;                 // the same, written down
let tax: Decimal<2, RoundHalfEven> = price * rate;     // narrowing, so the contract is required
let wrong: Decimal<2> = price * rate;                  // error: reaching Decimal<2> means rounding
```

**This makes the thesis sharper, not looser.** A contract now appears in a program exactly where
a value is narrowed, so its presence is information. Demanded everywhere it was ceremony, and
ceremony is what readers learn to ignore.

### The cost, stated: error recovery gets worse

`recover_from` exists because every `let` declared its type, so a statement whose *initializer*
was wrong still bound its name with the type the author asked for, and the rest of the function
checked against it instead of drowning the real error in "unknown name" noise. That advantage
is real and this decision gives up half of it: **an inferred binding whose initializer fails
has no type to recover with.**

Annotated bindings keep the better behaviour. That is a genuine argument for annotating in long
functions, it is recorded rather than discovered later, and it is the honest price of the
feature.

## 1b. Slice 2 — `for x in xs`

### Decision 1 — it iterates an array's elements, and it is a real statement

```text
for line in lines {
    print(line.render());
}
```

means exactly

```text
let mutable i = 0;
while i < len(lines) {
    let line = lines[i];
    i = i + 1;
    print(line.render());
}
```

`x` is a **copy** of the element, immutable, and scoped to the body — value semantics, the
same as every other binding. Writing to it is the ordinary immutability error, and writing to
the array through it is impossible, which is the point.

**Lowered in the back end, not the parser — and the first version got that wrong.** `+=` and
the field shorthand are parser desugars, so `for` was written as one too: a hidden `let mutable
for$i = 0;` and the loop above, with `$` chosen because no identifier may contain it. That
worked in stage-0 and is **impossible in stage-1**, because stage-1 names every binding by its
**span in the source** — and a synthesized index has no span. There is no byte sequence to
point at.

The options were to work around stage-1's representation or to accept that the representation
was right, and the second is true: a construct the two compilers implement two different ways
is a construct they can disagree about. So `for` is a statement in both, checked in both, and
lowered to the loop above in each back end.

It is a better design for a second reason. A parser desugar can only produce errors about what
it desugared *to*:

```text
for x in n { }        // n is an Int

error: len(...) needs an array or a string, but this has type Int      // the desugar
error: `for` iterates an array, and this is an Int                     // the statement
```

The author never wrote `len`.

**One thing the lowering must get right, and it cost a hung test.** The index advances
*before* the body, not after. `continue` jumps to the loop condition, so an increment at the
bottom is skipped and the loop never ends. A lowering has to be read against every
control-flow statement the language has, not just against the happy path.

### Decision 2 — `for` and `in` are reserved words

This language prefers **contextual** keywords, and `allocates`, `requires`, `ensures`,
`decreases` and `scaled` are all recognised only where nothing else can appear — so `let
allocates: Int = 0;` is legal. `for` does not qualify for that treatment: it opens a statement,
and a statement may also open with an identifier, so recognising it would need three tokens of
lookahead to tell `for x in xs` from `format(x);`.

Reserving them costs nothing in surprise, because every reader of every language already
expects `for` and `in` to be reserved — which is the actual test, not consistency with a rule
whose reason does not apply here.

### Decision 3 — the iterable is a name or a field path, and nothing else

```text
for x in xs { }              // a binding
for item in self.items { }   // a field path
for c in chunks_of(text) { } // refused
```

```text
error: `for` iterates a named array, and this is a call: its result would be
       recomputed on every pass. Bind it first — `let items = chunks_of(text);`
       — and iterate that.
```

The loop reads the iterable once per element — for its length and for the element — so
anything with a cost or an effect would pay it per pass. A name and a field path are free to re-read; a call is
not. Refused with the fix rather than silently made quadratic — which is the mistake M9 spent
four versions finding in this compiler's own source.

### Decision 4 — no index, no range, no `for` over anything else

`for x in xs` gives the element. If you need the position, `while` is still there and still
reads fine. A range form (`for i in 0..n`) is a second construct with its own type questions,
and `while i < n` already says it — deferred with a trigger below.

## 1c. What slices 1 and 2 must NOT do



- **NO inferring a parameter or return type.** A signature is the contract between a function
  and everyone who calls it, and a contract that has to be computed is not one you can read.
- **NO inferring a record field's type.** Same reason, plus layout is a fact about the type.
- **NO `var`, `auto`, or `:=`.** `let` already means "bind this"; a second spelling would be a
  second way to mean one thing.
- **NO declare-now-initialize-later.** `let x;` has no type and no value, and every language
  that allows it grows a definite-assignment analysis to compensate.
- **NO inference that crosses statements.** If the type needs two lines of context, the reader
  needs the annotation.
- **NO relaxing shadowing.** A second `let x` is still an error; inference changes nothing
  about which names exist.
- **NO inferring `allocates` or a contract.** Already refused by
  [M1a §2](M1a-CALLER-REGION-FUNCTIONS.md) and [A5](A5-CONTRACTS.md), and inference is not a
  reason to revisit it.
- **NO `for` over a String.** A String is bytes, and `byte_at` says "byte" precisely so the
  byte-versus-character question cannot hide. `for c in text` would hide it.
- **NO mutating the loop variable, and no writing through it.** It is a copy, and value
  semantics is not negotiable for a convenience.
- **NO `for` that evaluates its iterable more than once.** See slice 2, Decision 3.

## 2b. Slice 2b — the grammar swept against the bar (v0.0.95)

The bar is not a filter on new features, it is a lens for the **whole grammar**: for any
construct, what does the Python or PHP equivalent cost to write? If Burxt costs more and the
extra buys no correctness, the extra is the bug. Swept once, deliberately, and it found three
things — plus two that looked like gaps and were not.

**Fixed:**

1. **A trailing comma is allowed everywhere.** It was allowed in record and enum
   *declarations* and refused in parameter lists, argument lists, array literals, payloads,
   match bindings and type-argument lists. Refusing it makes adding an item a two-line diff
   and buys nothing.
2. **`function (self) name()` inside an `implement`.** The header already said which type, and repeating
   it on every method meant a five-method trait wrote the type six times. Outside an `implement`
   the annotation stays required, because there nothing else says it.
3. **Block comments get a real answer instead of a stray-token error.** `/* ... */` reported
   *"expected statement, found `/`"* from two tokens later. It now says Burxt has line
   comments only, and why that is a choice: one way to write a comment means no nesting rule
   to get wrong, and every editor will `//` a selection.

**Checked and already fine**, recorded so the next sweep does not re-check them: a call kept
for its effect (`push(xs, 1);` needs no binding), negative literals (`-1`, `-19.99`), the
field shorthand, `+=`, interpolation, `else if`, and `len` over both strings and arrays.

**Deliberately still absent**, each with the reason rather than an omission:

| Missing | Why it stays missing |
|---|---|
| `%` for modulo | `%` is the percent literal, and `8.25%` is a headline of this language. `remainder`, `divide_floor` and `divide_toward_zero` also *name* which convention is meant, which one operator cannot |
| Multi-line string literals | A literal spanning lines makes its own indentation part of the data — the one thing that surprises everybody about them. `\n` and `+` cover it |
| Block comments | See above: one way to write a comment |
| Default parameter values, named arguments | Real friction, real feature. Not refused on principle — just not built. Earns its place when a signature in this repo wants one |
| `to_string` of a record | Needs a display trait with a name the language blesses, which is a decision, not a shorthand |

## 2c. Slice 2c — every keyword is the word it means (v0.0.98)

Andre asked why a function is `fn` and a structure is `struct`, coming from PHP where both are
spelled out. The answer was not a good one: `fn`, `mut`, `impl`, `dyn`, `extern` and `struct`
were **inherited from Rust**, because Rust is where the memory model and the type discipline
came from. That is a habit, not a decision — and it sat badly next to the rest of the list:

| Spelled out | Clipped |
|---|---|
| `let` `return` `while` `for` `in` `if` `else` `match` `trait` `region` `print` `pure` `break` `continue` `allocates` `requires` `ensures` `decreases` | `fn` `mut` `impl` `dyn` `extern` `struct` |

**Twenty-five words against six.** And the rule had already been decided once, in the other
direction: `RoundHalfEven`, not `HalfEven`, because the self-explanatory spelling wins. So the
clippings were the ones out of step.

| Old | New | Why that word |
|---|---|---|
| `fn` | `function` | a function is a function |
| `mut` | `mutable` | a binding that can change is mutable |
| `impl` | `implement` | `implement Priced for Book` reads as the sentence it is |
| `dyn` | `dynamic` | the decision it names is made dynamically, at run time |
| `extern` | `external` | the function it names is external to this program |
| `struct` | `record` | named fields, copied by value, no inheritance, no hidden header |

`enum` is unchanged: it is short for enumeration, but every language spells it `enum` and
`enumeration` reads worse rather than clearer.

### Why `record` and not `structure`, `blueprint` or `capsule`

I first argued `struct` should stay because it is "a whole word". **That was wrong** — it is a
clipping of *structure*, which puts it in exactly the category being fixed.

- **`blueprint`** describes a *class*: a plan you manufacture instances from. A Burxt record has
  no constructor and no factory. The word would promise machinery that is not there.
- **`capsule`** implies encapsulation, and a record has none: every field is public, there is no
  `private`, and the layout is exactly the fields. It would name the opposite of the guarantee.
- **`structure`** is the literal unclipping, and it is longer without being clearer — jargon in a
  way `record` is not.
- **`record`** is what the thing *is*. In C#, Java, F# and Pascal it means precisely: named typed
  fields, value semantics, no inheritance. A PHP reader has no `struct` in their vocabulary and
  does know what a record is.

### The old spellings are reserved, and their only job is to say so

A clean break, not two ways to write one thing — "one obvious way to write each construct" is
the standing rule, and `fn` *or* `function` would be the kitchen sink.

```text
error: Burxt spells this `record`, not `struct`: named fields, copied by value, with no
       inheritance and no hidden header — which is what a record is, and what a class is
       not. Every keyword in this language is the word it means — which is why
       `allocates` and `decreases` are not `alloc` and `dec`.
```

A rename a reader cannot see the reason for is a rename they will resent, so each message
carries its reason.

## 2d. The tooling is part of the language (v0.0.99)

Andre reported three things after the rename: squiggles on files that compile, squiggles on
files that are not negatives, and `function` losing its colour. All three were true, none was a
compiler bug, and the reason they all happened at once is the rule this section exists to write
down.

> **A change to the language is not finished until the highlighter, the language server and the
> packaged extension have changed with it.** A reader's first contact with Burxt is an editor,
> not `cargo test`. A language that compiles correctly and looks broken *is* broken.

### The three, and what each one really was

**1. The editor was colouring yesterday's language.** `burxt-0.1.3.vsix` had been built before
the rename, so the installed grammar knew `fn` and had never heard of `function`. Nothing in the
repository noticed, because nothing was looking. Now
`the_packaged_extension_matches_the_grammar_in_the_repository` reads the grammar back out of the
`.vsix` and refuses a stale one.

**2. The grammar had never learned `function (self)`.** The receiver shorthand shipped in
v0.0.95; the method pattern still demanded `self: Type`, so six declarations across the examples
highlighted as nothing. `editor_grammar_knows_every_keyword_the_compiler_does` passed the whole
time, because **a keyword list is not a grammar** — it checks vocabulary, not whether the
language can be read. `editor_grammar_highlights_every_declaration_the_examples_write` now takes
every declaration line out of the real examples and requires some pattern to match it.

**3. A file is not always a program.** `examples/burxt/check.bx` is one of five modules
`examples/stage1.bx` assembles, and checking it alone reports every type declared in a sibling as
unknown — five files of squiggles that were not mistakes. Worse, `stage1.bx` itself reported a
parse error **on its own `use` lines**, because the language server never resolved imports at
all: a bug that had been there since modules shipped in v0.0.81 and that no test could see,
because `burxt check` resolves them and only the editor did not.

The fix is the design M6 already chose, applied one layer up: **check the program, keep the
diagnostics that landed in this file.** If the open file has `use` lines it is a root and is
loaded as one; if it has none, the nearby directories are searched for a program whose `use`
closure reaches it. Either way the concatenated buffer and its source map do the rest — the
editor's unsaved text is spliced over the file's own span, so what the user is looking at is what
gets checked. `the_language_server_checks_the_program_a_file_belongs_to` drives the real server
over stdio for every example, the standard library and every compiler module, and requires
`"diagnostics":[]` from all of them. `examples/negative/` is excluded on purpose: those are meant
to be wrong, and a squiggle there is the point.

### What this costs, stated

Checking a module now loads its whole program on every keystroke. For this compiler that is
200 KB and 1.1 seconds' worth of *compilation* — but the language server does the front end only,
which is a few milliseconds, and it is what M9 bought. If it ever bites, the answer is to cache
the program per root and re-check incrementally, not to go back to checking files.

## 2e. The rule reaches the compiler's own names (v0.0.103)

Andre asked why a type is `Ty`, and then said why he dislikes it: **`ty` already reads as
"thank you" or "typo".** That is a sharper argument than mine and it generalises — it is the same
test that chose `record` over `capsule`:

> **A name that reads as the wrong thing is worse than a name that is merely short.** An
> abbreviation colliding with a common meaning does not save characters, it spends a beat of
> confusion at every occurrence.

`fn`/`mut`/`struct` were the language's *keywords*, and v0.0.98 fixed those. `Ty` is an
identifier in the compiler's own source — which is the reference Burxt program and the most-read
Burxt in existence, so the rule reaches it too.

| Old | New | Why |
|---|---|---|
| `Ty` | `Type` | reads as the wrong thing; `type` was never a reserved word |
| `ty_of` | `declared_type` | the type a written **annotation** denotes |
| `ty_unknown` `ty_simple` `ty_decimal` | `unknown_type` `simple_type` `decimal_type` | constructors, named for what they build |
| `ty_same` + `types_same` | `same_type` | one function, not two |
| `ty_show` + `show_ty` | `show_type` | one function, not two |

**What kept `Ty` short was a collision, not a preference.** `type_of` already existed — the
expression typer — so the *annotation* reader could not have it, and past-me took the leftover
short name instead of finding a better pair. `declared_type` versus `type_of` says which is which
in a way neither `ty_of` nor `type_of` ever did: one reads what was **written down**, the other
works out what an **expression** is.

**And it cleaned up a smell I had made myself.** Two versions earlier I needed arena access and
added a *method* beside each free function, naming them `types_same`/`ty_same` and
`show_ty`/`ty_show` — near-homophones distinguished by nothing meaningful. The only callers of
each free version turned out to be the method that delegated to it, so both merged away. One
concept, one function.

445 occurrences across five files, fixpoint intact.

## 2f. The clip audit — four agents searching, one hand fixing (v0.0.104)

Andre asked whether any clipped names had been overlooked, and had four agents sweep disjoint
areas in parallel while the fixes went through one pair of hands. That division is the right one:
searching is embarrassingly parallel, and editing the same tree from four directions is not.

**The rule they applied, refined into two parts.** A *clipping* of one word is bad — `ty`, `rem`,
`arg`, `str`, `fs`. An *initialism read as itself* is not — `OS`, `IR`, `LSP`, `JSON`, `argc`.
That distinction settles most cases without argument: `os_` stays, `string_` does not.

### Fixed in this version

**Five builtins**, which are language surface a user types:

| Old | New | Why |
|---|---|---|
| `rem` | `remainder` | reads as REM sleep, R.E.M., BASIC's comment, the CSS unit |
| `div_floor` | `divide_floor` | `div` reads as an HTML tag |
| `div_trunc` | `divide_toward_zero` | `trunc` is not a word, and this says what it *does* — while avoiding `divide_truncate`, since `truncate` already means shortening an array |
| `arg` | `argument` | not a word; the AST already spelled it out |
| `arg_count` | `argument_count` | same |

`len` and `push` stay: Python, Go, Rust and JavaScript establish them, and neither can be
misread.

**88 test fixtures renamed**, because their file names still taught the vocabulary v0.0.98
removed — `struct_nested.bx`, `fn_money.bx`, `impl_receiver_shorthand.bx`, `dyn_dispatch.bx`,
`extern_duplicate.bx`, `mut_parameters_are_not_a_thing.bx`. The `old_keyword_*` fixtures keep
their names: those exist to test the retired spellings.

**`qty` → `quantity`**, 79 occurrences, most of them in `invoice.bx` — the flagship example, and
the one most likely to be read first.

**Two real defects**, found by an agent while reading rather than by any test: a dead binding in
`lib/string.bx` (`let trimmed: Int = 0;` — an `Int` named for a String operation, never read), and a
doc comment that had drifted to `sep` while its parameter said `separator`.

**Two test names** that described the language with words it no longer has.

### The mistake, recorded

The builtin sweep **broke the build** — 54 errors. `\barg\b` with word boundaries matched Rust's
own `Command::arg(...)` in 129 places. Exactly the class of error `\nfn` was in v0.0.98, and I
made it again one version after writing that lesson down. A word-boundary rename is only safe
when the word cannot occur in the *host* language, and `arg` occurs everywhere in Rust.

The three test failures after the revert were all downstream and honest ones: the editor grammar
listed the old builtin names, and four `panic` fixtures quoted them in expected output. Both are
the tooling rule from v0.0.99 doing its job.

### Finished in v0.0.106

All four decisions went the way §2f recommended.

**`enum` stays, with its reason written into the lexer** rather than left as an oversight: the
full word is longer *and* inaccurate, because a Burxt enum is a sum type whose variants carry
values, not an enumeration of integers. `choice` would be accurate and is named as the honest
alternative. That is a weaker defence than `record` had over `struct`, and the comment says so.

**`Err` → `Error`**, in Burxt only — in Rust, `Err` is `std::Result`'s own variant. The compiler
blesses the failing variant *by name* for `?`, so that string moved too.

**`str_` → `string_`, `fs_` → `file_`**, including the file names (`lib/string.bx`,
`lib/files.bx`) and `fs_make_dir` → `file_make_directory`. `os_` stays: an initialism read as
itself, established by Python, Go and Node.

**The compiler's own names**, roughly 900 replacements across five files:

| | |
|---|---|
| Records | `Tok`→`Token`, `Kw`→`Keyword`, `Sym`→`Binding`, `Fun`→`DeclaredFunction`, `Impl`→`Implementation`, `TypeDecl`→`TypeDeclaration`; `Typed` **deleted** (one occurrence — its own definition) |
| Foreign vocabulary | `no_struct`→`no_record_literal`, `parse_struct`→`parse_record`, `current_fn`, `fn_entry`, `open_fn`/`close_fn`, `parse_fn`, `dyn_method`, `emit_dyn` |
| Opaque fields | `toks`→`tokens`, `kids`→`children`, `subs`→`nested_children`, `tok`→`token`, `ret`→`return_type`, `words`→`keywords`, `next_tmp`→`next_register`, `in_mul`→`in_multiply`, the `n_*` counters |
| The exact failure mode | `spans_eq`→`spans_equal`, `span_eq`→`span_equal`, and the last `ty` fields → `bound_type` / `type_node` / `payload_type` |
| One-letter methods | `w`→`write_body`, `w_out`→`write_module`, `w_entry`→`write_entry` |
| 136 discarded results | `let e: Int = self.complain(...)` → `let complained: Int = ...` |

`DeclaredFunction` rather than `Function`: a record differing from the `function` keyword only in
case is a trap, not a name.

### Three self-inflicted bugs, and the rule they share

Every one was a word-boundary rename hitting a context the pattern could not see:

1. **`\barg\b` matched Rust's `Command::arg(...)`** — 129 sites, 54 build errors (v0.0.104).
2. **`Result.Err` matched inside the already-renamed `Result.Error`**, producing `Result.Erroror`.
3. **`\bret\b` matched the LLVM `ret` instruction** inside emitted-IR string literals, so the
   compiler produced invalid IR and the fixpoint broke.

The rule, which the first two versions of it did not state generally enough:

> **A word-boundary rename is only safe when the word cannot occur in any OTHER language the file
> contains.** These files hold three: Burxt, Rust, and LLVM IR as string data. `arg` is Rust's,
> `ret` is LLVM's, and a replacement can collide with its own output.

All three were caught by the suite in under a minute. That is the argument for a fast full run
between batches rather than one sweep at the end — six batches, six runs, and every failure
localised to the batch that caused it.

### Left for a decision

- **`enum`** is now the only clipped keyword, and the standing exception to v0.0.98's own rule —
  I defended it as universal, which is the same defence I rejected for `struct`. `enumeration` is
  worse (longer *and* inaccurate: a Burxt enum is a sum type). `choice` would be accurate.
- **`Err` → `Error`.** Two agents disagreed: one called it the exact failure mode ("err" as a
  hesitation noise), the other called it ecosystem convention that `?` matches on. It is
  genuinely both.
- **`string_` → `string_`, `file_` → `file_`** in the standard library, including the file names.
- **The compiler's own internals** — `Tok`, `Kw`, `Sym`, `Fun`, `Impl`, `Node.tok` (207 uses),
  `w` (185), `spans_eq`, `no_struct`, the `n_*` counters — plus stage-0's `ty` (344) and its AST
  still spelling `StructDef`/`ImplBlock`/`ExternFn`. Large, mechanical, and best done one name per
  commit with a full test run between.

## 3. Deferred, with triggers

| Feature | Why deferred | Earns its place when |
|---|---|---|
| `if` as an expression | Needs a rule for the type of a branchy value, and a `let` plus two assignments says it today | A real program reads worse for the lack of it than for the extra rule |
| Closures / arrow functions | No ownership story for captured state, which is the whole point of regions | Regions can express "this closure's captures live here" |
| `let` destructuring (`let Point { x, y } = p;`) | Sugar over field reads, and patterns exist only in `match` | Aggregate returns make multi-value binding common |
| Inferring a generic's type argument | That is [M7](M7-GENERICS.md)'s job, at the call site, not `let`'s | M7 lands |
| `for i in 0..n` (ranges) | A range is a second construct with its own type and its own questions, and `while i < n` says it today | A program reads worse for the lack of it, or ranges earn their place as values |
| `for (i, x) in xs` (the index too) | Needs tuples or a second binding form; `while` covers it | Tuples exist |
| `for` over a growable array being pushed to inside the loop | The bound is re-read each pass, so it works — but nobody should rely on that | Never; it is a bug waiting, and `while` makes the intent visible |

## 4. Acceptance

### Slice 2

1. `for` over a fixed array, a growable array, a field path, and nested — all working in both
   compilers, with `break` and `continue` behaving.
2. An empty array runs the body zero times.
3. Refused, each naming its fix: a non-array, a String, a call as the iterable, assigning to
   the loop variable, and a name already in scope.
4. **The compiler's own source uses it**, and the fixpoint still holds. An ergonomics feature
   its own author does not write has not landed.

### Slice 1

1. `let x = e;` and `let mutable x = e;` work for **every** type Burxt has: Int, Bool, String,
   Decimal with and without a contract, record, enum, `dynamic`, a built String, and the result of
   a call.
2. An array literal with no annotation is refused with Decision 2's message.
3. A scale mismatch downstream of an inferred binding is still an error — Decision 3's example
   is a `fail` fixture.
4. **Both compilers**, and the differential test still passes: stage-1 must parse and check an
   inferred `let` too, not merely tolerate one.
5. **The fixpoint still holds**, byte for byte.
6. Hover in the editor reports the inferred type, since that is where the annotation went.
7. `examples/` gains a program that uses it, the guide documents it, and at least one existing
   example is rewritten to show the difference — an ergonomics feature nobody writes in the
   examples has not landed.

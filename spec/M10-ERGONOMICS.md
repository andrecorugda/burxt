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
| `%` for modulo | `%` is the percent literal, and `8.25%` is a headline of this language. `rem`, `div_floor` and `div_trunc` also *name* which convention is meant, which one operator cannot |
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

# Burxt — the ergonomics that make it usable (M10)

> Status: **slice 1 DONE (v0.0.91).** `let x = e;` works in both compilers, the fixpoint
> holds, and the rounding rule got more correct on the way — a contract is now required
> exactly where a value narrows. Slice 2 (`for x in xs`) is next.
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
| 2 | `for x in xs { }` — the loop everyone writes, without the index | **next** |
| 3 | Generics ([M7](M7-GENERICS.md)) | specified |
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

**Signatures stay explicit.** Parameters, return types, struct fields, `allocates`, `pure` and
every contract are written down. Inference is local to one statement, so a reader never has to
look past the line in front of them to know what a binding holds — and the places a *reader of
someone else's code* needs types most are exactly the places that keep them.

This was flagged in `spec/README.md` at v0.0.18 as deserving its own decision rather than being
smuggled in with `$19.99`. This is that decision.

### Decision 2 — the annotation stays legal, and an array always names its type

```text
let xs: [Int; 3] = [1, 2, 3];        // fixed
let mut lines: [String] = [];        // growable
```

```text
error: an array literal does not say whether the array is fixed or growable, so an
       array binding names its type: `let xs: [Int; 3] = [1, 2, 3];` for a fixed
       one, or `let mut xs: [Int] = [];` for one that grows.
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

## 2. What slice 1 must NOT do

- **NO inferring a parameter or return type.** A signature is the contract between a function
  and everyone who calls it, and a contract that has to be computed is not one you can read.
- **NO inferring a struct field's type.** Same reason, plus layout is a fact about the type.
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

## 3. Deferred, with triggers

| Feature | Why deferred | Earns its place when |
|---|---|---|
| `if` as an expression | Needs a rule for the type of a branchy value, and a `let` plus two assignments says it today | A real program reads worse for the lack of it than for the extra rule |
| Closures / arrow functions | No ownership story for captured state, which is the whole point of regions | Regions can express "this closure's captures live here" |
| `let` destructuring (`let Point { x, y } = p;`) | Sugar over field reads, and patterns exist only in `match` | Aggregate returns make multi-value binding common |
| Inferring a generic's type argument | That is [M7](M7-GENERICS.md)'s job, at the call site, not `let`'s | M7 lands |

## 4. Acceptance for slice 1

1. `let x = e;` and `let mut x = e;` work for **every** type Burxt has: Int, Bool, String,
   Decimal with and without a contract, struct, enum, `dyn`, a built String, and the result of
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

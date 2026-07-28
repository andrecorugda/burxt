# Burxt — Generics (M7)

> Status: **slices 1–3 DONE in stage-0 (v0.0.93–v0.0.96)** — generic **functions**, generic
> **enums**, and **bounds**, monomorphised, with type arguments inferred at the call site and
> no turbofish required. Acceptance 8 is met: `Option<T>` and `Result<T, E>` are in `lib/`,
> written in Burxt, with no compiler support beyond this milestone. What remains: generic
> **records**, and stage-1. Stage-1 does not read generics yet,
> which is why the tests for this slice live in `tests/runner.rs` rather than `tests/pass/` —
> the same staging M5 used, with the second implementation following behind a ratchet.
>
> Original status: **specified, to implement.** Ordered before `Option`/`Result` (M8) deliberately:
> without generics those would be `OptionInt`, `OptionString`, `OptionDecimal` — code that
> generics would immediately delete. With generics, `Option<T>` is four lines of library.

## 0. What has to become possible

```text
record List<T> { items: [T] }

function (mutable self: List<T>) add(item: T) -> Int { push(self.items, item); return len(self.items); }

function largest<T: Ordered>(xs: [T]) -> T
    requires len(xs) > 0
{
    let mutable best: T = xs[0];
    let mutable i: Int = 1;
    while i < len(xs) { if xs[i] > best { best = xs[i]; } i = i + 1; }
    return best;
}
```

A framework is reusable abstraction, and today every container has to be written once per
element type. This is the piece that changes that.

## 1. Decisions

### Decision 1 — monomorphisation, not type erasure

Each instantiation becomes its own function or type at compile time: `largest<Int>` and
`largest<Decimal<2>>` are two functions in the object file. No boxing, no vtable, no runtime
type information, and a `List<Int>`'s elements are `Int`s in memory rather than pointers to
them.

**Why.** Burxt's promises are all about what a value *is* — a `Decimal<2>` is a scaled i64,
a record has no hidden header, `dynamic` costs nothing unless written. Erasure would put a
pointer where the value was and quietly undo that. The cost is code size, which is a
measurable and local problem; erasure's cost is a representation nobody asked for.

`dynamic Trait` already exists for the cases that genuinely want one implementation over many
types. Generics are for the cases that want many.

### Decision 2 — bounds are traits, and they are required

```text
function largest<T: Ordered>(xs: [T]) -> T
```

A type parameter with no bound can only be stored, copied and passed. To compare it, print
it or add it, the parameter must say so with a trait — and the compiler checks the body
against the bound rather than against each instantiation.

**Why required rather than inferred.** A generic function whose constraints are whatever its
body happens to do is a function whose signature is a lie: adding a `>` inside it silently
narrows every caller. Bounds make the contract the signature, which is the same argument
`allocates` and rounding contracts already make.

Two traits ship with this milestone, because the checker needs them for its own operators:
`Ordered` (`<`, `<=`, `>`, `>=`) and `Equatable` (`==`, `!=`). `Int`, `Decimal<S>`, `String`
and `Bool` implement them where the language already allows those operators.

### Decision 3 — one parameter list, on the declaration

`function name<T, U>(...)`, `record Name<T> { ... }`, `enum Name<T> { ... }`, and a method's
receiver names the type's parameters (`function (self: List<T>) first() -> T`). No generic
methods with their own extra parameters in this slice — a method may use its type's
parameters and nothing more.

### Decision 4 — instantiations are collected, then emitted

The checker walks the program and records every `(generic, type arguments)` pair it sees,
including ones reached through other generics. The backend emits one copy per pair, with the
type parameter substituted, and names it by mangling: `largest.Int`, `List.Decimal_2`.

A pair that is never used is never emitted, so a library may declare generics nobody
instantiates at no cost.

### Decision 5 — no specialisation, no variance, no HKTs

One definition per generic, applied uniformly. No `implement<T> Trait for List<T>` overlapping
with `implement Trait for List<Int>`; no covariance rules; no generic parameters that are
themselves generic. Each of those is a language of its own, and none is needed to write a
container.

## 2. What this must NOT do

- **NO type erasure.** See Decision 1.
- **NO inferred bounds.** See Decision 2.
- **NO implicit instantiation of a bound.** If `T: Ordered` is declared, the caller's type
  must implement `Ordered` — the compiler will not derive it because the comparison
  "happens to work".
- **NO generic `external function`.** C has no notion of it, and a monomorphised C symbol is a
  symbol that does not exist.
- **NO turbofish requirement.** `largest(xs)` infers `T` from the argument; an explicit
  `largest<Int>(xs)` is allowed where inference is ambiguous, and required nowhere else.
- **NO code-size surprise.** `burxt layout` grows a line per instantiation so the cost is
  visible rather than discovered in a binary.

## 3. Deferred, with triggers

| Feature | Why deferred | Earns its place when |
|---|---|---|
| Generic methods with their own parameters | A method using its type's parameters covers containers | A required program needs `function (self: List<T>) map<U>(...)` |
| Specialisation | Two rules for one call site is a language of its own | A measured hot path needs a hand-written case |
| Variance | Needs subtyping, which Burxt does not have beyond `dynamic` | Subtyping arrives |
| Higher-kinded parameters | No container of containers is needed yet | Someone writes a monad and can defend it |
| `where` clauses | One bound per parameter reads fine | A parameter needs three bounds and the line wraps |
| A generic over a fixed array's LENGTH | `[T; N]` would need N as a value parameter, which is a second kind of generic | A program needs one container over several fixed sizes; `[T]` covers it today |
| Type arguments written out (`largest<Int>(xs)`) | Inference from the arguments has covered every case so far | A parameter appears only in the return type and the call is worth writing |

## 3b. What slice 1 built, and what it turned out to need

**Inference is one function, one direction, no backtracking.** `unify(declared, actual)`
walks the two types together and binds a parameter to whatever stands opposite it. That is
the whole of it — no unification variables, no constraint set — and it is why `largest(xs)`
needs no turbofish and why the rule fits on a page.

**A generic's body is checked once, with its parameters standing for nothing.** That is what
puts the error at the declaration instead of at every call site, and it is the reason bounds
are required rather than inferred. An unbounded parameter can be stored, copied, passed and
returned; anything more is refused with a message naming the parameter and saying a bound is
how to allow it.

**Instantiation is a work list drained to a fixpoint**, not a single pass — because checking
one instantiation can discover another. A generic calling a generic works by binding the
inner parameter to the *outer* parameter while the outer is still abstract, and recording
nothing: the copy appears when the enclosing generic is instantiated and its body finally
names a concrete type. A runaway (a generic that reaches itself at a new type every pass) is
refused after 64 rounds with the reason, rather than compiled until the machine gives up.

**An instantiation is substituted in the AST, not threaded through the checker.** `specialise`
clones the declaration, replaces every parameter in the signature and in every `let`
annotation in the body, names it `identity$Int`, and hands it to exactly the code that checks
every other function. No second checking path means no second path that can disagree with the
first.

### Slice 2 — generic enums, and what made them cheap

`enum Option<T> { None, Some(T) }` works, and so does `Result<T, E>`. The mechanism is the one
slice 1 built, applied to types: **an application is rewritten into the nominal type of its
instantiation.** `Option<Int>` becomes `Option$Int`, a perfectly ordinary enum with its own
layout, and after that rewrite *no rule in the checker knows generics exist* — `match`,
exhaustiveness, payload binding, layout and codegen all see what they have always seen.

The rewrite happens in a pre-pass over the AST (`expand_program`), and again after each
function instantiation, because substituting `T := Int` can turn `Option<T>` into `Option<Int>`.

Three things it needed that functions did not:

**Inference has a second source.** `Option.Some(3)` infers `T` from the payload. `Option.None`
carries nothing, so it can only come from the context — which means an instantiation has to
remember what it was made from, so `Option$Int` can be read back as `(Option, [Int])` when a
declared type says what a variant does not.

**A generic's `match` is checked generically.** Inside `function or_else<T>(o: Option<T>, ...)` the
scrutinee's type is still `Option<T>`: its *variants* are known even though `T` is not, so the
arms, the exhaustiveness and the bindings are all checked once at the declaration. Both paths
share one `check_match_arms`, so an instantiation cannot be checked differently from the
declaration that produced it.

**The mangled name must never be shown.** A reader did not write `Option$String` and should not
learn that it exists, so every message pretty-prints an instantiation back to `Option<String>`.
That is a small function and it is not optional: the alternative is a language whose errors talk
about its own implementation.

**One surprise worth recording:** `function first<T>(xs: [T])` does not accept a `[Int; 3]`, and
should not. `[T]` is a growable array and `[Int; 3]` is a fixed one — different types with
different storage, exactly as [M10 §1b](M10-ERGONOMICS.md) says of `for`. A generic over a
fixed array's *length* is a different feature, deferred above.

### Slice 3 — bounds, and a promise kept

Every refusal on an unbounded parameter already ended with *"say so in the signature with a
bound on `T`"*. That was a promise the compiler could not keep for three versions, which is
the worst kind of error message. It keeps it now.

```text
function largest<T: Ordered>(a: T, b: T) -> T
function same<T: Equatable>(a: T, b: T) -> Bool
function describe<T: Priced>(item: T) -> String allocates    // any declared trait
```

**The two built-in bounds mirror exactly what the language already allows.** `Ordered` is
`Int` and `Decimal<S>`, because those are the types `<` works on today; `Equatable` adds
`Bool` and `String`, because those are the types `==` works on. Strings have no ordering yet,
so `Ordered` does not claim them — **a bound cannot promise more than the language delivers**,
and when Strings gain an order they gain it in one place.

**A trait bound gives static dispatch.** `describe<Book>` and `describe<Meal>` are two
functions; there is no vtable and no runtime type information. `dynamic Priced` remains for the
opposite case — one implementation serving many types at run time. Generics are for when many
implementations should serve one shape.

**Two checks, in two places, and that is the whole design.** The BODY is checked against the
bound, once, at the declaration — so adding a `>` inside a generic is a compile error until
the signature says so, rather than a silent narrowing of every caller. The ARGUMENT is checked
where the type is chosen, naming the parameter, the bound and the fix:

```text
error: `describe` needs `T: Priced`, and `Book` does not implement it. Write
       `implement Priced for Book { ... }` — conformance is declared, never inferred from
       having the right method names.
```

## 4. Acceptance

1. `function identity<T>(x: T) -> T` compiles, and `identity(3)` and `identity("s")` both work.
2. `record List<T>` with a `push`/`len` pair works for `Int` and for a record element.
3. ✅ `function largest<T: Ordered>(a: T, b: T) -> T` compiles; calling it with a type that does not
   implement `Ordered` is refused, naming the bound. A declared trait works as a bound too,
   with static dispatch.
4. A generic used from inside another generic is instantiated correctly (`largest<T>` called
   from `summarise<T: Ordered>`).
5. Two instantiations of one generic have **separate layouts**: `List<Int>` and
   `List<Decimal<4>>` differ where the element does, and `burxt layout` shows both.
6. An unused generic emits no code — checked by looking for the symbol in the IR.
7. Both compilers implement it and the differential test passes.
8. ✅ `Option<T>` and `Result<T, E>` (M8) are written **in Burxt, as library types**, with no
   compiler support beyond what this milestone provides. That is the test of whether the
   generics are real, and they passed it: `lib/option.bx` and `lib/result.bx`, four lines of
   declaration each, checked by `the_standard_library_compiles_and_works`.

# Burxt — Generics (M7)

> Status: **DONE (v0.0.111).** Generic **functions**, **records**, **enums**, **bounds** and
> **methods on a generic type**, in **both** compilers, with type arguments inferred at the call
> site and no turbofish anywhere. Stage-1 compiles 101 of the 102 pass programs; the one left
> needs `write_bytes`. `Option<T>` and `Result<T, E>` live in `lib/`, written in Burxt, needing no
> compiler support beyond this milestone.
>
> The tests live in `tests/pass/`, which is the claim that matters: a fixture there is held
> against **both** compilers end to end. They lived in `tests/runner.rs` while stage-1 could not
> read generics, and moving them was the acceptance criterion for saying this is done.
>
> The design in one line: **a type parameter is a question, not a placeholder.** Answer it at
> every point that asks and almost nothing has to be substituted — §"Where it actually stands
> (v0.0.111)" gives the two exceptions and why they are cheap.
>
> Original status: **specified, to implement.** Ordered before `Option`/`Result` (M8) deliberately:
> without generics those would be `OptionInt`, `OptionString`, `OptionDecimal` — code that
> generics would immediately delete. With generics, `Option<T>` is four lines of library.

## 0. What has to become possible

```text
class List<T> { items: [T] }

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

`function name<T, U>(...)`, `class Name<T> { ... }`, `enum Name<T> { ... }`, and a method's
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

### Slice 4 — generic records, and a bug that hid behind `Display`

`class Stack<T> { items: [T] }` with methods, which is what generics were for. The record half
is the enum mechanism unchanged: a concrete application is rewritten into the nominal type of
its instantiation, and after that nothing knows generics exist.

**Methods are the new part.** A method on a generic record is *held back* at registration — its
receiver has no layout until a use names the arguments — and one copy is made per instantiation,
so `Stack<Int>` and `Stack<String>` get their own `push_one`. The instantiation runs **twice**,
idempotently: once before any body is checked, because a body may call such a method; and again
in the drain loop, because a body can be what discovers a new instantiation. Missing the first
call costs `Stack$Int has no method named push_one`, which is precisely what it cost.

**Two things worth recording.**

`parse_fn_signature` *replaced* the type parameters in scope rather than extending them. A method's
receiver has already put the record's parameters in scope — `self: Stack<T>` — so replacing them
turned `item: T` from `Param("T")` into `Named("T")`. Both print as `T`, so a debug dump of the
specialised method looked *correct* while the substitution silently did nothing. The lesson is
about `Display`: two types that render identically are two types that a print statement cannot
tell apart, and the instrumentation has to name the variant, not the value.

Inferring a record's arguments from its field values is better than expected — `Holder { one: 1 }`
needs no annotation. But probing *every* field is wrong: `Stack { items: [] }` cannot type `[]`
alone, and asking it to try produced an error about array literals in the middle of an unrelated
rule. The probe now stops once every parameter is bound, and a field that cannot type itself is
skipped rather than propagated.

## 5. What stage-1 needs, and why it is not a transcription

Stage-1 cannot copy stage-0's design, for the reason `for x in xs` already ran into: **stage-1
names every type and binding by its SPAN in the source.** `Ty` is five integers — kind, scale,
contract, and the name's start and length. Stage-0 monomorphises by rewriting `Option<Int>` into
a nominal type called `Option$Int`, and **that name has no span**. There is no byte sequence to
point at.

So the design has to differ, and the shape that fits stage-1's representation is:

- **A generic application needs no new `Ty` field at all** — which the design got wrong before
  it was built. For a named type (`kind: 46`), `scale` and `contract` are *unused*, and a slice
  already stores its element as a **node index** in `scale`. So `Option<Int>` is a named type
  whose `scale` is the start of its argument nodes in `subs` and whose `contract` is how many:
  the same trick `kids` and `subs` already use, and one that reuses two dead fields instead of
  widening a record that is copied at every comparison in the checker.
- **A type parameter is `kind: 50`**, carrying the span of its own name. `T` is written down in
  the source, so it has a real span — which is the whole reason stage-1 can hold generics
  without inventing a name it could not point at.
- **Comparison becomes structural** for applications: `ty_same` recurses through the arguments
  instead of comparing one span.
- **Mangling moves to the emitter**, which already builds strings freely, so the LLVM symbol name
  is computed where symbol names belong and nowhere else.

### Where it actually stands (v0.0.111) — **M7 IS DONE**

**Both compilers now check and emit every generic form the language has**: functions, bounded
functions, records, enums, and methods on a generic type. Stage-1 compiles **101 of the 102 pass
programs**, and the one left needs `write_bytes` — nothing to do with generics.

The last piece was methods, and it is the one case where **the copy is chosen by the RECEIVER
rather than by the arguments**. `Pair<Point>.left` needs no unification at all: the call site
already knows what it is holding. So the symbol is `Pair$Point.left`, discovery is "note the
receiver's application", and the drain does the rest — a work-list entry whose generic turns out
to be a record or an enum means *every method on it* wants a copy.

**The defect worth remembering is not a wrong answer, it is a wrong FRAME.** `is_aggregate` did
not resolve, so a method returning `T` where `T = Point` did not set up an sret — while the call
site, reading a type the checker had already resolved, passed a storage pointer as the first
argument. Two halves of one call disagreeing about the shape of a function. It segfaulted, which
is the better failure: a printed wrong number would have been found much later. Every
calling-convention decision in the emitter now resolves first, through one named predicate
(`is_aggregate_written`) rather than fifteen call sites each remembering to.

That is the third time this milestone that the fix was **resolve where you read** rather than
**rewrite before you start** — after `same_type` and after `cells_of`. Worth stating as the
design rule it turned out to be:

> A type parameter is a question, not a placeholder. Answer it at every point that asks, and
> nothing ever has to be substituted. The only exceptions are the two places that must hand back
> something self-contained — a nested application's argument list, and a function BODY — and both
> of them are cheap because they fire only when something actually changed.

**Left in M7: nothing.** Higher-kinded types, variance and specialisation are not in this
milestone and are not obviously wanted; if they arrive they arrive with a program that needs them.

### Where it stood (v0.0.110)

**Generic functions are emitted by both compilers, and stage-1 now compiles 100 of the 101 pass
programs** (the one left needs `write_bytes`, nothing to do with generics). This is the piece
that genuinely needed monomorphisation, and it is worth being precise about why, because the
rest of generics did not:

> A **layout** is a count, and a count can be recomputed under different bindings.
> A **body** is instructions, and `identity<T>` is a load for `T = Int` and a memcpy for
> `T = Point`. There is no one function to emit, so there has to be one per argument list.

Four pieces, and the shape is stage-0's:

1. **`mangle_type`** — a type as a symbol fragment, total by construction. Every kind answers
   something, because a kind that fell through to `""` would collide two instances into one
   definition and the module would define a function twice.
2. **`find_instance`** — find-or-add, so two calls with the same arguments share one copy.
3. **`instance_of_call`** — the type arguments worked out the way the checker worked them out:
   unify the declared parameter types against the actual ones. The actuals come from the
   checker's own cache and are **resolved deeply first**, because inside `echo<T>` the recorded
   type of the argument is `T` and what this needs is the Int that `T` stands for right here.
   Answers -1 when an argument is still abstract, which is not a failure — it means the
   enclosing generic has not been instantiated yet, and the call will be reached again when it is.
4. **The work list**, drained after `main` rather than before it. A `while` over `len`, re-read
   each turn: emitting `echo$Int` discovers that `identity$Int` is wanted, so the list grows
   while it is being walked. It terminates because a program writes finitely many argument lists
   and `find_instance` makes each one entry. After `main` because before it the list is empty —
   nothing has been walked yet, and LLVM does not mind a call to a function defined further down.

`echo$Int` calling `identity$Int` is the case that proves the work list, and the test greps for
a call to an *unmangled* generic, because that is what a missed suffix looks like: a link error
rather than a wrong answer.

**What is left is METHODS on a generic type.** `Stack<T>.push` wants a mangled receiver as well
as a mangled name, and a receiver that is itself an application. `emit_module` names that and
refuses it; stage-0 emits it. That is the last item in M7.

### Where it stood (v0.0.109)

**Generic records and enums are EMITTED**, by both compilers, and they needed no
monomorphisation at all — which was the surprise. In Burxt a layout is a **count of eight-byte
cells**, not a named machine type: everything a value can be is eight bytes wide or an aggregate
of things that are. So `cells_of` and `offset_of` read the one declaration under the arguments in
scope and answer 2 for `Pair<Int>` and 4 for `Pair<Point>`. No copy, no mangled type, no arena
entry. The prediction in §5 — that layout "cannot be lazy" — was wrong, and wrong in the same
direction as the substitution prediction before it. Twice now the design assumed a rewrite and
the answer was a resolution.

Four defects, each of which the previous one hid:

1. **Size right, field type wrong.** The emitter asked whether a `T` was an aggregate, was told
   no, and stored a Point's *address* where its two cells belonged: a record of exactly the
   right size holding a pointer in the wrong place. Field types are now resolved under the
   holder's arguments in one named function, `field_type_here`, because check.bx's record-literal
   rule already did exactly this and two copies of a rule is one too many.
2. **A generic inside a generic.** `Nested<Point>.inner` is written `Pair<T>`, and whoever
   receives that type has no bindings left. `resolve_deep` now descends into an application's
   arguments and lays down a fresh argument list when one changed — the single place laziness
   runs out, and it costs entries only when it fires.
3. **An inner binding shadowed an outer one.** Binding `Pair`'s `T` to the literal `T` hid
   `Nested`'s `T = Point`, because `resolve_shallow` searches backwards and stops at the first
   match. A binding is now **resolved before it is pushed**, so the innermost is already the
   answer. `Pair<Point>` was 2 cells instead of 4 for exactly this reason.
4. **The payload rule is about the instantiation, not the declaration.** `Option<T>`'s payload is
   a parameter, which is neither scalar nor aggregate until an argument says. Checked once per
   application, with the arguments in scope, and it now reads the same sentence stage-0 reads.

`tests/pass/generics_layout.bx` pins all of it, and pins it the right way: it reads the fields
back out and adds them rather than only building the value, because three of the four defects
above produced a value of exactly the right size.

**What is left is generic FUNCTIONS in the emitter.** That one genuinely needs a copy: a body
whose parameter may be one cell or four compiles to different code, so it wants a mangled symbol
per instantiation, a work list, and call sites that name the copy. `emit_module` refuses exactly
that and says so; stage-0 emits them. Two pass fixtures wait on it, both named in the backend
ratchet's comment.

### Where it stood (v0.0.108)

**The whole front end checks generics — functions, records and enums — in both compilers**, and
the generics tests now live in `tests/pass/`, which is the bar that matters: a fixture in
`tests/pass/` is held against *both* compilers by the differential test, while a test written
inline in `tests/runner.rs` is only ever held against whatever it asserts. `generics_types.bx`
and `generics_functions.bx` moved there, and both compilers run them to the same output.

Four things had to be right for the second one, and each was a distinction I could not see until
a fixture failed:

1. **A generic calling a generic.** `function echo<T>(x: T) -> T { return identity(x); }` binds
   `identity`'s `T` to `echo`'s `T` — still abstract, and correct: the concrete type appears when
   `echo` is instantiated. The check that every parameter be *settled* was asking what the
   parameter resolved to, and an unbound parameter and one bound to another parameter both
   resolve to kind 50. It now asks the binding table whether a binding **exists**
   (`has_binding`), because existence is the question and resolution never was.
2. **Arity and genericness over every application**, in one sweep of the arena rather than at
   each type position — a rule spread over every position is a rule with a hole in it.
3. **A generic `external function` is refused**: C has no type parameters, so there would be no
   symbol to link against.
4. **A generic named with nothing to infer from** — `let nothing = Option.None;` — says to write
   the type, and shows the annotation that would fix it.

What remains is layout: giving `Option<Int>` its own record or enum in the **emitter**, which is
genuinely one copy per argument list and cannot be done lazily. `emit_module` holds that refusal
now, moved out of `check()` because checking works and only layout does not. One fail fixture,
`generic_enum_payload_must_be_scalar`, is knowingly out of scope until then, and the ratchet
comment in `tests/runner.rs` names it rather than absorbing it.

### Where it stood (v0.0.107)

**Generic functions are checked.** Type parameters are bound at the call site by structural
unification, resolved **lazily** as comparison recurses, and their bounds enforced — so stage-1
agrees with stage-0 on generic functions.

The design got simpler again on contact, and the simplification is the interesting part:
**nothing is substituted.** A `[T]` is never rewritten into a `[Int]`, because a slice holds its
element as a node index and a substituted element would have no node — the wall §5 predicted.
Instead a binding table maps a parameter's *span* to a type, and `resolve_shallow` is called at
each level of every recursive walk. `same_type` resolves as it descends, so `[T]` against
`[Int]` matches without either being rewritten. No new arena entries, no synthesized names, and
the wall turned out to be avoidable rather than merely climbable.

What remains is layout: giving `Option<Int>` its own record or enum, which is genuinely one copy
per argument list and cannot be done lazily. That is the next slice, and the guard now refuses
exactly that and nothing more.

### Where it stood (v0.0.101)

**The parser is done.** Type parameters on functions, records and enums, bounds, generic
receivers (`self: Stack<T>`) and applications in any type position all read with zero parse
errors. Parameters go in scope as **token indices** and are compared by bytes, like every other
name in stage-1; `parse_item` truncates the list at each declaration boundary rather than each
parser exit, for the reason `commit(base)` gives about child lists — dozens of exits are dozens
of chances to forget.

**The checker refuses, and says why.** It does not monomorphise yet, and the honest intermediate
state is a refusal naming the milestone — not a crash, and not silent acceptance. Before the
guard existed, stage-1 parsed a generic, walked a type-parameter node, looked its name up as a
record, got -1 and indexed an array with it: exit 70. A compiler that half-understands a
construct answers differently from the other one, which is what the differential test exists to
prevent.

`the_burxt_compiler_reads_generics_and_says_it_cannot_check_them` pins both halves: the parser
must stay complete, and the checker must keep saying so — with a message telling whoever removes
it to move the generics tests into `tests/pass/`, where both compilers are held to them.

That is a genuinely different implementation of one language, which is what the differential test
exists to police: the two must agree on what they accept and what they answer, never on how.

**Acceptance for that half, stated so it can fail:** the positive generics tests move out of
`tests/runner.rs` and into `tests/pass/`, where both compilers are held to them — and the
compiler's own source uses a generic, with the byte-identical fixpoint intact. Until then the
ratchet stays honest rather than widened.

## 4. Acceptance

1. `function identity<T>(x: T) -> T` compiles, and `identity(3)` and `identity("s")` both work.
2. `class List<T>` with a `push`/`len` pair works for `Int` and for a record element.
3. ✅ `function largest<T: Ordered>(a: T, b: T) -> T` compiles; calling it with a type that does not
   implement `Ordered` is refused, naming the bound. A declared trait works as a bound too,
   with static dispatch.
4. A generic used from inside another generic is instantiated correctly (`largest<T>` called
   from `summarise<T: Ordered>`).
5. ✅ Two instantiations have **separate layouts**: `Pair<Int, String>` and
   `Pair<Decimal<2>, Bool>` differ where their fields do.
6. An unused generic emits no code — checked by looking for the symbol in the IR.
7. Both compilers implement it and the differential test passes.
8. ✅ `Option<T>` and `Result<T, E>` (M8) are written **in Burxt, as library types**, with no
   compiler support beyond what this milestone provides. That is the test of whether the
   generics are real, and they passed it: `lib/option.bx` and `lib/result.bx`, four lines of
   declaration each, checked by `the_standard_library_compiles_and_works`.

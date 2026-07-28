# Burxt — Generics (M7)

> Status: **specified, to implement.** Ordered before `Option`/`Result` (M8) deliberately:
> without generics those would be `OptionInt`, `OptionString`, `OptionDecimal` — code that
> generics would immediately delete. With generics, `Option<T>` is four lines of library.

## 0. What has to become possible

```text
struct List<T> { items: [T] }

fn (mut self: List<T>) add(item: T) -> Int { push(self.items, item); return len(self.items); }

fn largest<T: Ordered>(xs: [T]) -> T
    requires len(xs) > 0
{
    let mut best: T = xs[0];
    let mut i: Int = 1;
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
a struct has no hidden header, `dyn` costs nothing unless written. Erasure would put a
pointer where the value was and quietly undo that. The cost is code size, which is a
measurable and local problem; erasure's cost is a representation nobody asked for.

`dyn Trait` already exists for the cases that genuinely want one implementation over many
types. Generics are for the cases that want many.

### Decision 2 — bounds are traits, and they are required

```text
fn largest<T: Ordered>(xs: [T]) -> T
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

`fn name<T, U>(...)`, `struct Name<T> { ... }`, `enum Name<T> { ... }`, and a method's
receiver names the type's parameters (`fn (self: List<T>) first() -> T`). No generic
methods with their own extra parameters in this slice — a method may use its type's
parameters and nothing more.

### Decision 4 — instantiations are collected, then emitted

The checker walks the program and records every `(generic, type arguments)` pair it sees,
including ones reached through other generics. The backend emits one copy per pair, with the
type parameter substituted, and names it by mangling: `largest.Int`, `List.Decimal_2`.

A pair that is never used is never emitted, so a library may declare generics nobody
instantiates at no cost.

### Decision 5 — no specialisation, no variance, no HKTs

One definition per generic, applied uniformly. No `impl<T> Trait for List<T>` overlapping
with `impl Trait for List<Int>`; no covariance rules; no generic parameters that are
themselves generic. Each of those is a language of its own, and none is needed to write a
container.

## 2. What this must NOT do

- **NO type erasure.** See Decision 1.
- **NO inferred bounds.** See Decision 2.
- **NO implicit instantiation of a bound.** If `T: Ordered` is declared, the caller's type
  must implement `Ordered` — the compiler will not derive it because the comparison
  "happens to work".
- **NO generic `extern fn`.** C has no notion of it, and a monomorphised C symbol is a
  symbol that does not exist.
- **NO turbofish requirement.** `largest(xs)` infers `T` from the argument; an explicit
  `largest<Int>(xs)` is allowed where inference is ambiguous, and required nowhere else.
- **NO code-size surprise.** `burxt layout` grows a line per instantiation so the cost is
  visible rather than discovered in a binary.

## 3. Deferred, with triggers

| Feature | Why deferred | Earns its place when |
|---|---|---|
| Generic methods with their own parameters | A method using its type's parameters covers containers | A required program needs `fn (self: List<T>) map<U>(...)` |
| Specialisation | Two rules for one call site is a language of its own | A measured hot path needs a hand-written case |
| Variance | Needs subtyping, which Burxt does not have beyond `dyn` | Subtyping arrives |
| Higher-kinded parameters | No container of containers is needed yet | Someone writes a monad and can defend it |
| `where` clauses | One bound per parameter reads fine | A parameter needs three bounds and the line wraps |

## 4. Acceptance

1. `fn identity<T>(x: T) -> T` compiles, and `identity(3)` and `identity("s")` both work.
2. `struct List<T>` with a `push`/`len` pair works for `Int` and for a struct element.
3. `fn largest<T: Ordered>(xs: [T]) -> T` compiles; calling it with a type that does not
   implement `Ordered` is refused, naming the bound.
4. A generic used from inside another generic is instantiated correctly (`largest<T>` called
   from `summarise<T: Ordered>`).
5. Two instantiations of one generic have **separate layouts**: `List<Int>` and
   `List<Decimal<4>>` differ where the element does, and `burxt layout` shows both.
6. An unused generic emits no code — checked by looking for the symbol in the IR.
7. Both compilers implement it and the differential test passes.
8. `Option<T>` and `Result<T, E>` (M8) are written **in Burxt, as library types**, with no
   compiler support beyond what this milestone provides. That is the test of whether the
   generics are real.

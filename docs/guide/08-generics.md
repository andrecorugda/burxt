---
title: Generics
---

# 8. Generics

One definition, one copy per type that uses it.

## The problem

You write a function that returns the larger of two `Int`s. Then you need it for `Decimal<2>`.
Every language answers this somehow, and the answer says a lot about what it values:

| | What you get |
|---|---|
| Copy the function | Two versions to keep in step, and a third when someone needs `String` |
| `interface{}` / `Object` / `any` | One version, and the type is gone — put anything in, cast on the way out |
| Erased generics (Java, TypeScript) | Types at compile time, a pointer at run time |
| **Monomorphised generics** | One source, and a separate machine function per type, each fully typed |

Burxt monomorphises. `identity<Int>` and `identity<String>` are two functions in the object
file, and a `Decimal<2>` inside a generic is **still a scaled i64** rather than a pointer to one.

That last sentence is the whole reason not to erase. Every other promise this language makes is
about what a value *is* — an exact decimal is an integer and a scale, a record is its fields laid
out in order, a region is a bump pointer. Erasure would put a pointer where the value was, and a
boxed `Decimal<2>` is not the thing the money guarantees are about.

## Writing one

```burxt
function identity<T>(x: T) -> T {
    return x;
}

region r {
    print(identity(3));         // T = Int
    print(identity("text"));    // T = String
}
```

The type parameter is named where the function is declared, in angle brackets after the name.

**There is no turbofish.** You never write `identity::<Int>(3)` or `identity<Int>(3)`, because the
argument already says what `T` is and asking twice is asking you to repeat yourself. Type
arguments are inferred at the call site by matching each declared parameter type against the
actual one:

```burxt
function first<T>(xs: [T]) -> T {
    return xs[0];
}
// `first(names)` where names is a [String] infers T = String from one level down
```

A parameter can be nested inside another type — `[T]`, `[T; 4]` — and inference descends through it.

## Records and enums

```burxt
record Pair<T> { first: T, second: T }
enum Holder<T> { Empty, Full(T) }
```

Written the same way, and used with the arguments filled in:

```burxt
region r {
    let small: Pair<Int> = Pair { first: 3, second: 4 };
    let wide: Pair<Point> = Pair { first: Point { x: 1, y: 2 },
                                   second: Point { x: 9, y: 8 } };
}
```

Those two are genuinely different shapes in memory. `Pair<Int>` is two cells; `Pair<Point>` is
four. Nothing is boxed to make them the same size, which is what "one copy per type" buys.

## Constructors

A function may build a generic and answer it, with the type arguments coming from **where the value
lands** rather than from an argument:

```burxt
record Bag<T> { items: [T], count: Int }

function empty_bag<T>() -> Bag<T> {
    return Bag { items: [], count: 0 };
}

region r {
    let mutable names: Bag<String> = empty_bag();   // T = String, from the annotation
    let numbers: Bag<Int> = empty_bag();            // T = Int, same source, second copy
}
```

`empty_bag()` takes no arguments at all, so there is nothing to infer `T` from — except the type
already written on the left. Reading it from there is why there is still no turbofish: writing
`empty_bag::<String>()` would be the language demanding an answer it is holding.

This works when every field can be built without a `T` in hand — an empty `[T]` needs no element.
A `Bag<T>` with a `first: T` field cannot be built out of nothing, and asking says so by name.

**A generic name always needs its arguments.** `let x: Pair = ...` is refused, and so is

```burxt
let nothing = Holder.Empty;
```

because a value has to know what it holds. The compiler says what to write:

```
error: `Holder.Empty` does not say what `T` is, and nothing here does. Write the type
       where the value lands — `let x: Holder<...> = Holder.Empty;` — or pass it
       somewhere that names it.
```

## Methods

A method on a generic type names the parameter in the receiver:

```burxt
function (self: Pair<T>) left() -> T {
    return self.first;
}

function (mutable self: Pair<T>) swap() -> Int {
    let keep: T = self.first;
    self.first = self.second;
    self.second = keep;
    return 0;
}
```

`Pair<Int>.swap` and `Pair<Point>.swap` are separately compiled, because for `Point` each of
those three assignments moves two cells instead of one. Same source, different instructions —
which is the point.

## Bounds

A bare `T` can be moved, stored and returned. It cannot be compared, added or printed, because
nothing said it could be. To ask for more, name a bound:

```burxt
function largest<T: Ordered>(a: T, b: T) -> T {
    if a > b {
        return a;
    }
    return b;
}
```

Two bounds ship, and each one is exactly a set the language already has:

| Bound | Means | Because |
|---|---|---|
| `Ordered` | `Int`, `Decimal` | the types `<` works on |
| `Equatable` | `Int`, `Decimal`, `Bool`, `String` | the types `==` works on |

**A bound cannot promise more than the language delivers.** There is no `Addable`, because `+` on
two `Decimal`s has a scale rule that a bound would have to lie about. When a bound is missing the
error names the operator, not the bound:

```
error: `largest` needs `T: Ordered`, and String has no order. Ordered is Int and
       Decimal — the types `<` works on.
```

Bounds are checked **where the type argument is chosen** — at the call site — so the error points
at the call that made the choice, not at the body that needed it.

## What is refused

- **`print(x)` on a bare `T`.** Printing needs to know how wide the value is and how to format
  it. Add a bound or take a `String`.
- **A generic `external function`.** C has no type parameters, so there would be no symbol to
  link against.
- **An ENUM as a generic enum's payload.** `Holder<Inner>` where `Inner` is itself an enum is
  refused: an enum inside an enum has no finite size without indirection, because the inner one's
  payload area would have to be big enough for the outer one. That is a memory question, not a
  layout one. A **record** payload — `Holder<Point>` — works, and it is checked per
  *instantiation*, because `Holder<T>`'s payload is neither one thing nor the other until an
  argument says.
- **Type arguments on a plain type**, and the wrong number of them. Both say so by name.

## Under the hood, and why you might care

Monomorphisation usually means *substitution*: copy the declaration and rewrite the types inside
it. Burxt mostly does not, and the reason is worth knowing if you are reading the compiler.

A layout in Burxt is a **count of eight-byte cells** — everything a value can be is eight bytes
wide or an aggregate of things that are. So `Pair<Int>` and `Pair<Point>` are read from *one*
declaration under different bindings, and no copy of the type exists anywhere. Only **bodies**
need copies, because `identity<T>` compiles to a load for `Int` and a memory copy for `Point`.

The rule the compiler follows: **a type parameter is a question, not a placeholder.** Answer it
at every point that asks, and almost nothing has to be substituted.

## Next

[Absence and failure](09-absence-and-failure.md) — `Option`, `Result`, and why there is no null.
Both of them are ordinary Burxt written with what this page describes, which was the test for
whether these generics are real.

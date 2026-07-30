---
title: Generics
---

# 9. Generics

## The problem, as it actually arrives

You write a function returning the larger of two `Int`s. A week later you need it for `Decimal<2>`.
Every language answers this, and the answer says what the language actually values:

<div class="tablewrap" markdown="1">

| | What you get |
|---|---|
| Copy the function | Two versions to keep in step, and a third when somebody needs `String` |
| `interface{}` / `Object` / `any` | One version, and the type is gone — put anything in, cast on the way out |
| Erased generics (Java, TypeScript) | Types while compiling, a pointer at run time |
| **Monomorphised generics** | One source, a separate machine function per type, each fully typed |

</div>

Burxt takes the last one, and for this page there is only one reason worth knowing: erasure would put
a **pointer** where the value was, and a boxed `Decimal<2>` is not the thing the money guarantees are
about. Everything Burxt promises is about what a value *is* — an exact decimal is an integer and a
scale, a class is its fields laid out in order, a region is a bump pointer. Hand that to a generic
and get a pointer back and you have handed away the whole design.

## Think of a stencil, not a box

A generic is a **stencil**. You cut it once, then stamp it into whatever material you like, and each
stamp comes out in that material — steel stays steel, paper stays paper. Erasure is the opposite: it
puts everything in the same cardboard box first so one stamp fits all of them.

<svg viewBox="0 0 640 254" role="img" aria-label="One generic source becomes a separate fully typed machine function per type" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .b { fill: #fff; stroke: #111; stroke-width: 1.5; }
    .t { font: 12px ui-monospace, monospace; fill: #111; }
    .g { font: 11px ui-monospace, monospace; fill: #888; }
    .s { font: 11px ui-monospace, monospace; fill: #b00; }
    .a { stroke: #111; stroke-width: 1.5; fill: none; marker-end: url(#a9); }
    @media (prefers-color-scheme: dark) {
      .b { fill: #1b1b1b; stroke: #ddd; } .t { fill: #eee; } .s { fill: #ff8080; }
      .a { stroke: #ddd; } .g { fill: #999; }
    }
  </style>
  <defs>
    <marker id="a9" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/>
    </marker>
  </defs>

  <text class="g" x="20" y="24">one source</text>
  <rect class="b" x="20" y="92" width="164" height="52" rx="4"/>
  <text class="t" x="32" y="114">identity&lt;T&gt;(x: T)</text>
  <text class="g" x="32" y="132">-&gt; T</text>

  <text class="g" x="330" y="24">one machine function each</text>

  <rect class="b" x="330" y="36" width="290" height="52" rx="4"/>
  <text class="t" x="342" y="58">bx.identity$Int</text>
  <text class="g" x="342" y="76">i64 in, i64 out — one cell</text>

  <rect class="b" x="330" y="102" width="290" height="52" rx="4"/>
  <text class="t" x="342" y="124">bx.identity$Decimal_2_</text>
  <text class="s" x="342" y="142">i64 in, i64 out — STILL a scaled integer</text>

  <rect class="b" x="330" y="168" width="290" height="52" rx="4"/>
  <text class="t" x="342" y="190">bx.identity$Point</text>
  <text class="g" x="342" y="208">sret / byval — two cells, by value</text>

  <path class="a" d="M184 108 L326 62"/>
  <path class="a" d="M184 118 L326 128"/>
  <path class="a" d="M184 128 L326 194"/>

  <text class="g" x="20" y="240">erasure would give one function and a pointer in all three rows</text>
</svg>

Those are not an illustration — they are the symbol names, and you can go and look:

```sh
$ burxt emit-ir app.bx | grep define
define i64 @"bx.identity$Int"(i64 %0)
define i64 @"bx.identity$Decimal_2_"(i64 %0)
define void @"bx.identity$Point"(ptr sret(%bx.Point) %0, ptr byval(%bx.Point) %1)
```

The middle line is the one that matters. A `Decimal<2>` inside a generic is an `i64` — the same
scaled integer it is everywhere else, with the same exactness and the same refusals.

## Writing one

```burxt
function identity<T>(x: T) -> T {
    return x;
}

print(identity(3));         // T = Int
print(identity("text"));    // T = String
```

The type parameter is named where the function is declared, in angle brackets after the name.

**There is no turbofish.** You never write `identity::<Int>(3)` or `identity<Int>(3)`, because the
argument already says what `T` is and asking twice is asking you to repeat yourself. Type arguments
are inferred by matching each declared parameter type against the actual one, and inference descends
through nesting — `[T]`, `[T; 4]`:

```burxt
function first<T>(xs: [T]) -> T {
    return xs[0];
}

let mutable names: [String] = [];
let n: Int = push(names, "ada");
print(first(names));        // T = String, from one level down
```

## Classes and enums

```burxt
class Point { x: Int, y: Int }
class Pair<T> { first: T, second: T }
enum Holder<T> { Empty, Full(T) }

let small: Pair<Int> = Pair { first: 3, second: 4 };
let wide: Pair<Point> = Pair { first: Point { x: 1, y: 2 },
                               second: Point { x: 9, y: 8 } };
print(small.first);
print(wide.second.y);
```

Those two are genuinely different shapes in memory. `Pair<Int>` is two cells; `Pair<Point>` is four.
Nothing is boxed to make them the same size — which is exactly what one-copy-per-type buys.

## Building one out of nothing

Type arguments can also come from **where the value lands** rather than from an argument:

```burxt
class Bag<T> { items: [T], count: Int }

function empty_bag<T>() -> Bag<T> {
    return Bag { items: [], count: 0 };
}

let mutable names: Bag<String> = empty_bag();   // T = String, from the annotation
let numbers: Bag<Int> = empty_bag();            // T = Int, same source, second stamp
print(names.count);
print(numbers.count);
```

`empty_bag()` takes no arguments, so there is nothing to infer `T` *from* — except the type already
written on the left. Reading it from there is the same principle as the missing turbofish: writing
`empty_bag::<String>()` would be the language demanding an answer it is already holding.

This works when every field can be built without a `T` in hand — an empty `[T]` needs no element. A
`Bag<T>` with a `first: T` field cannot be built out of nothing, and asking says so by name.

**A generic name always needs its arguments.** `let x: Pair = ...` is refused, and so is this:

```burxt
enum Holder<T> { Empty, Full(T) }
let nothing = Holder.Empty;
```

```
error: `Holder.Empty` does not say what `T` is, and nothing here does. Write the type
       where the value lands — `let x: Holder<...> = Holder.Empty;` — or pass it
       somewhere that names it.
```

## Methods

A method on a generic type names the parameter in the receiver:

```burxt
class Pair<T> { first: T, second: T }

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

`Pair<Int>.swap` and `Pair<Point>.swap` are separately compiled, because for `Point` each of those
three assignments moves two cells instead of one. Same source, different instructions.

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

print(largest(3, 9));
print(largest($19.99, $4.50));
```

Two bounds ship, and each is exactly a set the language already has:

<div class="tablewrap" markdown="1">

| Bound | Means | Because |
|---|---|---|
| `Ordered` | `Int`, `Decimal` | the types `<` works on |
| `Equatable` | `Int`, `Decimal`, `Bool`, `String` | the types `==` works on |

</div>

**A bound cannot promise more than the language delivers.** There is no `Addable`, because `+` on
two `Decimal`s has a [scale rule](02-numbers-and-money.md) that a bound would have to lie about.

When a bound is missing, the error names the *operator* rather than the bound, because the operator
is the thing you were actually trying to use:

```
error: `largest` needs `T: Ordered`, and String has no order. Ordered is Int and
       Decimal — the types `<` works on.
```

Bounds are checked **where the type argument is chosen** — at the call site — so the error points at
the call that made the choice, not at the body that needed it.

## What is refused

- **`print(x)` on a bare `T`.** Printing has to know how wide the value is and how to format it. Add
  a bound, or take a `String`.
- **A generic `external function`.** C has no type parameters, so there would be no symbol to link
  against.
- **An enum as a *generic enum's* payload.** `Holder<Inner>` where `Inner` is itself an enum is
  refused: the inner one's payload area would have to be big enough for the outer one, so there is no
  finite size without indirection. A **class** payload — `Holder<Point>` — works, and it is checked
  per *instantiation*, because `Holder<T>`'s payload is neither one thing nor the other until an
  argument says which.
- **Type arguments on a plain type**, and the wrong number of them. Both say so by name.

## Under the hood, if you are reading the compiler

Monomorphisation usually means *substitution*: copy the declaration and rewrite the types inside it.
Burxt mostly does not.

A layout here is a **count of eight-byte cells** — everything a value can be is eight bytes wide, or
an aggregate of things that are. So `Pair<Int>` and `Pair<Point>` are read from *one* declaration
under different bindings, and no copy of the type exists anywhere. Only **bodies** need copies,
because `identity<T>` compiles to a load for `Int` and a memory copy for `Point`.

The rule the compiler follows: **a type parameter is a question, not a placeholder.** Answer it at
every point that asks, and almost nothing has to be substituted.
([The design record.](../../spec/M7-GENERICS.md))

## Next

[Absence and failure](10-absence-and-failure.md) — `Option`, `Result`, and why there is no null.
Both are ordinary Burxt written with exactly what this page describes, which was the test for whether
these generics are real.

---
title: Generics
description: A type parameter is a cookie cutter, not a box — one definition, one real machine function per type, nothing erased.
---

# 9. Generics

## What this is for
{: #what-this-is-for}

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

## Think of a cookie cutter
{: #think-of-a-cookie-cutter}

A cookie cutter is not a container. You do not put dough *in* it and get a generic cookie out. You press
it into whatever dough you have, and what comes out is a real cookie made of that dough — gingerbread if
the dough was gingerbread, shortbread if it was shortbread.

One cutter, three doughs, three real cookies. Not three references to a cookie-shaped idea.

<figure>
<svg viewBox="0 0 680 254" role="img" aria-label="One cookie cutter pressed into three different doughs gives three real cookies: a generic definition is compiled once per type that uses it, and nothing is erased or boxed" style="max-width:100%;height:auto;">
  <style>
    .cut  { fill: none; stroke: #1d1d1f; stroke-width: 2.4; }
    .dough{ fill: #0f6f3c; opacity: .10; }
    .edge { fill: none; stroke: #1d1d1f; stroke-width: 1.6; }
    .press{ fill: none; stroke: #0071e3; stroke-width: 2; marker-end: url(#mk); }
    .hair { fill: none; stroke: #d2d2d7; stroke-width: 1; }
    .h    { font: 600 13.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .t    { font: 11.5px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #1d1d1f; }
    .blue { font: 600 11.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #0071e3; }
    .cap  { font: 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
  </style>
  <defs>
    <marker id="mk" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="#0071e3"/>
    </marker>
  </defs>

  <text class="h" x="8" y="18">One cutter</text>
  <path class="cut" d="M30 44 h96 v70 h-96 z M30 60 h96 M46 44 v70"/>
  <text class="t" x="14" y="136">Stack&lt;T&gt;</text>
  <text class="cap" x="14" y="158">written once</text>

  <path class="press" d="M154 96 h48"/>
  <text class="blue" x="150" y="70">pressed</text>
  <text class="blue" x="150" y="86">into</text>

  <text class="h" x="226" y="18">Three doughs</text>
  <rect class="dough" x="226" y="36" width="120" height="34" rx="6"/>
  <rect class="edge"  x="226" y="36" width="120" height="34" rx="6"/>
  <text class="t" x="238" y="58">Int</text>
  <rect class="dough" x="226" y="82" width="120" height="34" rx="6"/>
  <rect class="edge"  x="226" y="82" width="120" height="34" rx="6"/>
  <text class="t" x="238" y="104">Decimal&lt;2&gt;</text>
  <rect class="dough" x="226" y="128" width="120" height="34" rx="6"/>
  <rect class="edge"  x="226" y="128" width="120" height="34" rx="6"/>
  <text class="t" x="238" y="150">Item</text>

  <path class="press" d="M362 53 h44"/>
  <path class="press" d="M362 99 h44"/>
  <path class="press" d="M362 145 h44"/>

  <text class="h" x="430" y="18">Three real functions</text>
  <rect class="edge" x="430" y="36" width="226" height="34" rx="6"/>
  <text class="t" x="440" y="58">Stack$Int.push</text>
  <rect class="edge" x="430" y="82" width="226" height="34" rx="6"/>
  <text class="t" x="440" y="104">Stack$Decimal2.push</text>
  <rect class="edge" x="430" y="128" width="226" height="34" rx="6"/>
  <text class="t" x="440" y="150">Stack$Item.push</text>

  <line class="hair" x1="8" y1="188" x2="672" y2="188"/>
  <text class="cap" x="8" y="212">Nothing is erased and nothing is boxed. Each one is a separate machine function over the real</text>
  <text class="cap" x="8" y="230">layout of its type — so a <tspan font-family="ui-monospace, monospace">Stack&lt;Int&gt;</tspan> holds integers, not pointers to integers.</text>
</svg>
<figcaption>A type parameter is a shape you press into a type, not a box you put a type into. That is why
<code>Option&lt;T&gt;</code> and <code>Map&lt;K, V&gt;</code> are ordinary library files rather than keywords.</figcaption>
</figure>

## A step closer
{: #a-step-closer}

## In code
{: #in-code}

### Writing one

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

### Classes and enums

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

### Building one out of nothing

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

### Methods

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

### Bounds

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
print(largest("apple", "pear"));
```

Two bounds ship, and each is exactly a set the language already has:

<div class="tablewrap" markdown="1">

| Bound | Means | Because |
|---|---|---|
| `Ordered` | `Int`, `Decimal`, `String` | the types `<` works on |
| `Equatable` | `Int`, `Decimal`, `Bool`, `String` | the types `==` works on |

</div>

**A bound cannot promise more than the language delivers.** There is no `Addable`, because `+` on
two `Decimal`s has a [scale rule](02-numbers-and-money.md) that a bound would have to lie about.

A `String` is ordered by its **bytes** — so `"Zebra"` comes before `"apple"`, because `Z` is 90 and `a`
is 97. That is not alphabetical order in any language, and it is deliberate: locale collation means
choosing a language *and* one of that language's several orders, which is a decision nobody wrote down.
Byte order is the one ordering that needs no decision and is identical on every machine, which is what
a sort has to be to stay reproducible.

When a bound is missing, the error names the *operator* rather than the bound, because the operator
is the thing you were actually trying to use:

```
error: `largest` needs `T: Ordered`, and Bool has no order. Ordered is Int,
       Decimal and String — the types `<` works on.
```

Bounds are checked **where the type argument is chosen** — at the call site — so the error points at
the call that made the choice, not at the body that needed it.

### What is refused

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

### Under the hood, if you are reading the compiler

Monomorphisation usually means *substitution*: copy the declaration and rewrite the types inside it.
Burxt mostly does not.

A layout here is a **count of eight-byte cells** — everything a value can be is eight bytes wide, or
an aggregate of things that are. So `Pair<Int>` and `Pair<Point>` are read from *one* declaration
under different bindings, and no copy of the type exists anywhere. Only **bodies** need copies,
because `identity<T>` compiles to a load for `Int` and a memory copy for `Point`.

The rule the compiler follows: **a type parameter is a question, not a placeholder.** Answer it at
every point that asks, and almost nothing has to be substituted.
([The design record.](https://github.com/andrecorugda/burxt/blob/main/spec/M7-GENERICS.md))

## Why it is built this way
{: #why-it-is-built-this-way}

**Because erasure would put a pointer where a value belongs.** A `Stack<Decimal<2>>` holds scaled
integers laid out end to end. If generics were erased it would hold pointers to boxed integers, and every
read would be a chase — which would make the exact-money story slower than the float story it replaces.

**Because it is the test of whether the generics are real.** `Option<T>` is four lines of Burxt with no
compiler support beyond generics. `Map<K, V>` is one file. If either had needed a keyword, the generics
were decoration — that was the bar set in `spec/M7-GENERICS.md`, and it is why the standard library looks
the way it does.

**Because a bound is a promise a reviewer can read.** `T: Ordered` in a signature says exactly which
types may arrive, in the place people already look.

## What it costs
{: #what-it-costs}

**One machine function per type that uses it.** Three instantiations are three copies in the binary. For
the sizes real programs reach this is the right trade; for a generic used at forty types it is forty
copies.

**Bounds are a short list.** `Ordered` is `Int` and `Decimal` — the types `<` works on. `Equatable` is
those plus `Bool` and `String`. You cannot define a bound of your own.

**No specialisation, no variance, no associated types, no higher-kinded anything.** A type parameter is a
type, and that is all it is.

**Type arguments come from the annotation when the arguments cannot settle them** — `let m: Map<String,
Int> = map_new();`. That reads well and it means a call whose result you do not bind sometimes has nothing
to infer from.

## When you reach for it
{: #when-you-reach-for-it}

<div class="tablewrap" markdown="1">

| You want | Write |
|---|---|
| a function over any type | `function f<T>(x: T) -> T` |
| a function that compares | `function f<T: Ordered>(a: T, b: T)` — `Int` and `Decimal` only |
| a function that uses `==` or a map key | `<K: Equatable>` |
| a container | a generic `class`, and methods on it |
| one value of several shapes, over any type | a generic `enum` — that is all `Option<T>` is |
| an empty container with nothing to infer from | annotate the binding: `let m: Map<String, Int> = map_new();` |

</div>

## Examples
{: #examples}

**One definition, two types, two real functions.**

```burxt
function largest<T: Ordered>(a: T, b: T) -> T {
    if a > b { return a; }
    return b;
}

print(largest(3, 9));
print(largest($19.99, $4.50));
print(largest("apple", "pear"));
```

```
9
19.99
pear
```

Those are three separate machine functions — one over `Int`, one over `Decimal<2>`, one over `String` —
and the `Decimal` one compares scaled integers directly rather than unboxing anything.

**And the bound doing its job.** `Bool` has no order — is `false` smaller than `true`, or is the
question meaningless? Nobody wrote it down, so it cannot arrive:

```burxt
function largest<T: Ordered>(a: T, b: T) -> T {
    if a > b { return a; }
    return b;
}

print(largest(true, false));
```

```
error: `largest` needs `T: Ordered`, and Bool has no order. Ordered is Int, Decimal and String — the types `<` works on.
 --> largest.bx:6:7
  |
6 | print(largest(true, false));
  |       ^^^^^^^^^^^^^^^^^^^^
```

The message names the bound, names the type that failed it, **and lists what the bound contains** — so
you do not have to go and look it up.

## Next
{: #next}

[Absence and failure](10-absence-and-failure.md) — `Option`, `Result`, and why there is no null.
Both are ordinary Burxt written with exactly what this page describes, which was the test for whether
these generics are real.

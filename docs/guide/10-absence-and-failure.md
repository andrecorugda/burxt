---
title: Absence and failure
description: Absence is a sealed envelope you have to open. There is no null, and match forces both cases.
---

# 10. Absence and failure

## What this is for
{: #what-this-is-for}

A function looks up a customer and returns one, or null if there is no such customer. Six places call
it. Five of them check. The sixth is on an error path that only runs when something *else* has already
gone wrong — so it is the least tested line in the file, and it is the one that dereferences null at
two in the morning during the incident it was supposed to help with.

Nobody was careless. The signature said `Customer`. It did not say *or nothing*, because in a language
with null there is no way for it to say that — **every reference is two things at once**, a value and
possibly nothing, while the type mentions only the first. The compiler cannot help, because as far as
it knows there is nothing to check.

Tony Hoare called null his billion-dollar mistake. The cost is not the ugliness; it is that the
information is not in the type.

## Think of a sealed envelope
{: #think-of-a-sealed-envelope}

Two envelopes arrive. One has a letter in it and one is empty, and from the outside they are identical.

You cannot read a letter you have not opened, and you cannot tell which envelope you are holding without
opening it. That is not an inconvenience — it is the only honest description of the situation, and every
language with `null` pretends otherwise by letting you *act* as though there is a letter and finding out
at three in the morning that there was not.

<figure>
<svg viewBox="0 0 680 262" role="img" aria-label="Option as two identical sealed envelopes, one holding a letter and one empty: you must open it to know, and match forces both cases to be written. Result is the same shape with a reason slip instead of nothing." style="max-width:100%;height:auto;">
  <style>
    .env  { fill: #ffffff; stroke: #1d1d1f; stroke-width: 1.8; }
    .flap { fill: none; stroke: #1d1d1f; stroke-width: 1.4; }
    .letter{ fill: #ffffff; stroke: #0f6f3c; stroke-width: 1.6; }
    .lfill{ fill: #0f6f3c; opacity: .10; }
    .slip { fill: #ffffff; stroke: #c8102e; stroke-width: 1.6; }
    .sfill{ fill: #c8102e; opacity: .08; }
    .rule { fill: none; stroke: #d2d2d7; stroke-width: 1; }
    .hair { fill: none; stroke: #d2d2d7; stroke-width: 1; }
    .h    { font: 600 13.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .t    { font: 11.5px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #1d1d1f; }
    .cap  { font: 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .grn  { font: 600 11.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #0f6f3c; }
    .red  { font: 600 11.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #c8102e; }
  </style>

  <text class="h" x="8" y="18">Option&lt;T&gt; — a letter, or nothing</text>

  <rect class="env" x="14" y="34" width="122" height="76" rx="5"/>
  <path class="flap" d="M14 34 l61 40 l61 -40"/>
  <rect class="lfill" x="30" y="82" width="90" height="20" rx="3"/>
  <rect class="letter" x="30" y="82" width="90" height="20" rx="3"/>
  <text class="grn" x="14" y="126">Some(value)</text>

  <rect class="env" x="162" y="34" width="122" height="76" rx="5"/>
  <path class="flap" d="M162 34 l61 40 l61 -40"/>
  <line class="rule" x1="178" y1="92" x2="268" y2="92"/>
  <text class="cap" x="162" y="126">None — empty</text>

  <text class="cap" x="14" y="156">Identical from outside. You</text>
  <text class="cap" x="14" y="174">must open it to know, and</text>
  <text class="cap" x="14" y="192">match will not compile</text>
  <text class="cap" x="14" y="210">unless you write both cases.</text>

  <line class="hair" x1="330" y1="8" x2="330" y2="240"/>

  <text class="h" x="368" y="18">Result&lt;T, E&gt; — a letter, or a reason</text>

  <rect class="env" x="374" y="34" width="122" height="76" rx="5"/>
  <path class="flap" d="M374 34 l61 40 l61 -40"/>
  <rect class="lfill" x="390" y="82" width="90" height="20" rx="3"/>
  <rect class="letter" x="390" y="82" width="90" height="20" rx="3"/>
  <text class="grn" x="374" y="126">Ok(value)</text>

  <rect class="env" x="522" y="34" width="122" height="76" rx="5"/>
  <path class="flap" d="M522 34 l61 40 l61 -40"/>
  <rect class="sfill" x="538" y="82" width="90" height="20" rx="3"/>
  <rect class="slip"  x="538" y="82" width="90" height="20" rx="3"/>
  <text class="red" x="522" y="126">Error(why)</text>

  <text class="cap" x="374" y="156">The same shape, except the</text>
  <text class="cap" x="374" y="174">second envelope carries a</text>
  <text class="cap" x="374" y="192">slip saying what went wrong.</text>
  <text class="cap" x="374" y="210">? passes it on unchanged.</text>

  <text class="cap" x="8" y="256">Both are ordinary library files. Neither is a keyword.</text>
</svg>
<figcaption>There is no <code>null</code>. Absence is a type, so "I forgot to handle missing" is a compile
error rather than a crash at three in the morning — and both envelopes are ordinary library files, four
lines of Burxt each, rather than anything the compiler knows about.</figcaption>
</figure>

## A step closer
{: #a-step-closer}

Both envelopes are **enums**, and that is the whole mechanism:

```burxt
enum Option<T> {
    None,
    Some(T)
}
```

An enum is one value that is one of several shapes, and `match` on one is refused unless every shape is
written. So there is no way to *act as though* the letter is there — not because a rule forbids it, but
because the value you are holding is the envelope, and the letter only exists inside a branch that
established there was one.

`Result<T, E>` is the same shape with a reason instead of nothing:

```burxt
enum Result<T, E> {
    Error(E),
    Ok(T)
}
```

## In code
{: #in-code}

### Option

```burxt
enum Option<T> {
    None,
    Some(T)
}
```

That is the whole definition, and where it lives matters: **`lib/option.bx` is a library file, not a
language feature.** Four lines of ordinary Burxt, written with the [generics](09-generics.md) from the
last page, with no compiler support of any kind. That was the test those generics had to pass — if
`Option` had needed a keyword, they were not real generics.

Opening the envelope:

```burxt
use "lib/option.bx";

function describe(found: Option<String>) -> Int {
    match found {
        None => { print("nothing"); }
        Some(value) => { print(value); }
    }
    return 0;
}

let n: Int = describe(Option.Some("ada"));
let m: Int = describe(Option.None);
```

**Both arms are required.** `match` is exhaustive, so *I forgot to handle missing* is a compile error
rather than the 2am incident at the top of this page. That is the entire mechanism. Everything below
is convenience.

Two conveniences worth naming:

```burxt
use "lib/option.bx";

let found: Option<String> = Option.None;
let name: String = option_or(found, "anonymous");   // the value, or a fallback
if option_is_some(found) { print("something"); }    // whether there is anything there
```

`option_is_some` deliberately does not hand you the value. **Asking is not the same as having** — a
function that did both would let you check one thing and read another.

### Result

```burxt
enum Result<T, E> {
    Ok(T),
    Error(E)
}
```

A function that can fail **says so in its type**:

```burxt
use "lib/result.bx";

function divide(a: Int, b: Int) -> Result<Int, String> {
    if b == 0 {
        return Result.Error("division by zero");
    }
    return Result.Ok(divide_toward_zero(a, b));
}
```

No exception to catch, no error code to forget, and nothing that can travel silently up eight frames
to a handler nobody wrote. `E` is usually a `String` while a program is young, and an enum once its
failures are worth naming.

### `?`

Matching every fallible call is correct, and quickly tedious. `?` handles the common shape: give me
the value, or **return the failure from the enclosing function right now**.

```burxt
use "lib/result.bx";

function divide(a: Int, b: Int) -> Result<Int, String> {
    if b == 0 { return Result.Error("division by zero"); }
    return Result.Ok(divide_toward_zero(a, b));
}

function halve_then_double(a: Int, b: Int) -> Result<Int, String> {
    let n: Int = divide(a, b)?;      // the value, or return the error immediately
    return Result.Ok(n * 2);
}
```

It works on `Option` the same way, in a function that answers with one:

```burxt
use "lib/option.bx";

function index_of(xs: [Int], wanted: Int) -> Option<Int> {
    let mutable i: Int = 0;
    while i < len(xs) {
        if xs[i] == wanted {
            return Option.Some(i);
        }
        i += 1;
    }
    return Option.None;
}

function neighbour(xs: [Int], wanted: Int) -> Option<Int> {
    let at: Int = index_of(xs, wanted)?;   // the index, or return None right now
    return Option.Some(at + 1);
}
```

`neighbour(xs, 8)` answers `Some(2)`; `neighbour(xs, 99)` answers `None` without the second line ever
running.

Here is the detail that makes it work at all: **`?` recognises failure by the VARIANT name** —
`Error`, or `None` — never by the enum's own name. Which is exactly what lets `Option` and `Result` be
library types rather than built-ins, and it means your own enum with a `None` or `Error` variant works
with `?` without asking anyone's permission.

## Why it is built this way
{: #why-it-is-built-this-way}

Three of those four refusals are the same move: **when a shortcut would hide a decision, the language
asks for the decision instead.**

That is what *friendly but never compromised* means in practice. `?` exists because writing `match`
twenty times is friction with no decision in it. `unwrap` does not exist because it is a decision
disguised as convenience. The test is never how much typing something saves — it is whether anything
was decided.

**And neither is a keyword.** `Option<T>` is four lines of Burxt in
[`lib/option.bx`](https://github.com/andrecorugda/burxt/blob/main/lib/option.bx); `Result<T, E>` is much
the same. That was the test set for whether [generics](09-generics.md) were real — if absence had needed
compiler support, they were not.

## What it costs
{: #what-it-costs}

**You write `match` where another language wrote a dereference.** Both cases, every time. That is the
whole bill, and it is charged at the point where the other language charged nothing and billed you later.

### What is deliberately absent

**There is no `unwrap`.** A function meaning *give me the value and abort if there is none* is null
with extra steps, and the crash it causes is precisely the one this type exists to prevent. `option_or`
covers the case where a default is right; `match` covers the case where it is not. Between them there
is no case left that wants a panic.

**There is no `map`.** It needs a function as a value, and a closure needs an owner for its captured
state — which is a [memory](04-memory.md) question, not a syntax one. Deferred with the reason
recorded, not forgotten.

**`?` does not convert between error types.** If the callee fails with a `String` and you fail with
something else, write the `match`. Somebody has to decide what the caller's failure *means*, and that
decision does not belong to an operator.

**A payload may not be another enum.** `Option<Point>` works — a class payload is fine — but
`Option<Inner>` where `Inner` is an enum is refused, because an enum inside an enum has no finite size
without indirection. Same reason as in [Generics](09-generics.md).

## When you reach for it
{: #when-you-reach-for-it}

<div class="tablewrap" markdown="1">

| The situation | Reach for |
|---|---|
| it might not be there, and that is ordinary | `Option<T>` — a missing key, an empty line |
| it might fail, and *why* matters | `Result<T, E>` |
| a missing value has an obvious default | `option_or(found, fallback)` |
| you only need to know whether it is there | `option_is_some(found)` |
| a chain of steps where the first failure should propagate | `f(x)?` — the failure returns unchanged |
| you want to check and then use | `match`. There is no `unwrap`, on purpose |

</div>

## Examples
{: #examples}

**Parsing text that might not be a number.** `string_parse_int` answers an `Option<Int>`, and `match`
forces both cases:

```burxt
use "lib/option.bx";
use "lib/string.bx";

function port_of(text: String, fallback: Int) -> Int {
    match string_parse_int(text) {
        None => { return fallback; }
        Some(n) => { return n; }
    }
}

print(port_of("8080", 80));
print(port_of("not a number", 80));
```

```
8080
80
```

No exception, no sentinel, no `-1` that a caller has to know about. The second call is not an error —
absence was one of the two answers, and the code says what to do about it.

## Next
{: #next}

[Maps](11-maps.md) — a key-value table in insertion order, which is where `Option` earns its keep: a
lookup that might find nothing is the commonest reason anyone reaches for one.

Or the running code: [`examples/absence.bx`](https://github.com/andrecorugda/burxt/blob/main/examples/absence.bx) for this page and
[`examples/generics.bx`](https://github.com/andrecorugda/burxt/blob/main/examples/generics.bx) for the one before. Both compile, and a test in
the suite makes sure they keep compiling.

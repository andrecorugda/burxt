---
title: Absence and failure
---

# 10. Absence and failure

## The problem, as it actually arrives

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

Burxt takes the other branch: **absence is a type.**

A `String` is a String — it is there, always. If it might not be, it is an `Option<String>`, which is
a *different* type: an envelope that either has a letter in it or does not. You cannot read the letter
without opening the envelope, and opening it means saying what happens in both cases.

<svg viewBox="0 0 640 266" role="img" aria-label="With null the type says one thing and run time has two; with Option the compiler makes you handle both" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .b { fill: #fff; stroke: #111; stroke-width: 1.5; }
    .gate { fill: none; stroke: #b00; stroke-width: 2.5; }
    .t { font: 12px ui-monospace, monospace; fill: #111; }
    .g { font: 11px ui-monospace, monospace; fill: #888; }
    .s { font: 11px ui-monospace, monospace; fill: #b00; }
    .a { stroke: #111; stroke-width: 1.5; fill: none; marker-end: url(#a10); }
    @media (prefers-color-scheme: dark) {
      .b { fill: #1b1b1b; stroke: #ddd; } .t { fill: #eee; } .s { fill: #ff8080; }
      .gate { stroke: #ff8080; } .a { stroke: #ddd; } .g { fill: #999; }
    }
  </style>
  <defs>
    <marker id="a10" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/>
    </marker>
  </defs>

  <text class="g" x="20" y="22">with null: the type says one thing, run time has two</text>
  <rect class="b" x="20" y="52" width="140" height="40" rx="4"/>
  <text class="t" x="32" y="77">String name</text>
  <path class="a" d="M160 72 L246 50"/>
  <path class="a" d="M160 72 L246 96"/>
  <rect class="b" x="250" y="34" width="110" height="32" rx="4"/>
  <text class="t" x="262" y="55">"ada"</text>
  <rect class="b" x="250" y="82" width="110" height="32" rx="4"/>
  <text class="s" x="262" y="103">null</text>
  <path class="a" d="M360 50 L436 50"/>
  <text class="g" x="444" y="54">fine</text>
  <path class="a" d="M360 98 L436 98"/>
  <text class="s" x="444" y="102">crash, at run time</text>

  <text class="g" x="20" y="160">with Option: the type says two, and the compiler makes you say both</text>
  <rect class="b" x="20" y="190" width="150" height="40" rx="4"/>
  <text class="t" x="32" y="215">Option&lt;String&gt;</text>
  <line class="gate" x1="210" y1="176" x2="210" y2="248"/>
  <text class="s" x="184" y="168">match</text>
  <path class="a" d="M170 210 L204 210"/>
  <path class="a" d="M216 210 L246 192"/>
  <path class="a" d="M216 210 L246 232"/>
  <rect class="b" x="250" y="174" width="150" height="32" rx="4"/>
  <text class="t" x="262" y="195">Some(value)</text>
  <rect class="b" x="250" y="222" width="150" height="32" rx="4"/>
  <text class="t" x="262" y="243">None</text>
  <text class="s" x="414" y="200">both arms required</text>
  <text class="g" x="414" y="216">at compile time</text>
</svg>

Nothing is implicitly absent, so **no dereference can fail**. There is no `nil`, no `undefined`, no
`NULL`, and no `""` standing in for missing.

## Option

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

## Result

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

## `?`

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

## What is deliberately absent

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

## The pattern worth taking away

Three of those four refusals are the same move: **when a shortcut would hide a decision, the language
asks for the decision instead.**

That is what *friendly but never compromised* means in practice. `?` exists because writing `match`
twenty times is friction with no decision in it. `unwrap` does not exist because it is a decision
disguised as convenience. The test is never how much typing something saves — it is whether anything
was decided.

## Next

[Maps](11-maps.md) — a key-value table in insertion order, which is where `Option` earns its keep: a
lookup that might find nothing is the commonest reason anyone reaches for one.

Or the running code: [`examples/absence.bx`](../../examples/absence.bx) for this page and
[`examples/generics.bx`](../../examples/generics.bx) for the one before. Both compile, and a test in
the suite makes sure they keep compiling.

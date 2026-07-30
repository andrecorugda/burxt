---
title: Absence and failure
---

# 9. Absence and failure

There is no null. No `nil`, no `undefined`, no `NULL`, and no `""` standing in for missing.

## What that actually removes

Tony Hoare called null his billion-dollar mistake, and the reason is not that null is ugly. It is
that **every reference becomes two things at once** — a value, and possibly nothing — while the
type says only the first. The compiler cannot help you, because as far as it knows there is
nothing to check.

Burxt takes the other branch: **absence is a type**. A `String` is a String. If it might not be
there, it is an `Option<String>`, and that is a different type which does not have `String`'s
methods until you have said what happens when it is missing.

Nothing is implicitly absent, so no dereference can fail.

## Option

```burxt
use "lib/option.bx";

enum Option<T> {
    None,
    Some(T)
}
```

That is the whole definition, and where it lives matters: **`lib/option.bx` is a library file, not
a language feature.** Four lines of ordinary Burxt, written with the generics from the previous
page, with no compiler support of any kind. That was the test the generics had to pass — if
`Option` had needed a keyword, they were not real generics.

Reading one:

```burxt
match found {
    None => { print("nothing"); }
    Some(value) => { print(value); }
}
```

**Both arms are required.** `match` is exhaustive, so "I forgot to handle missing" is a compile
error rather than a crash at three in the morning. That is the entire mechanism; everything else
is convenience.

The two conveniences worth naming:

```burxt
let name: String = option_or(found, "anonymous");   // the value, or a fallback
if option_is_some(found) { ... }                    // whether there is anything there
```

`option_is_some` deliberately does not give you the value. Asking is not the same as having.

## Result

```burxt
use "lib/result.bx";

enum Result<T, E> {
    Ok(T),
    Error(E)
}
```

A function that can fail **says so in its type**:

```burxt
function divide(a: Int, b: Int) -> Result<Int, String> {
    if b == 0 {
        return Result.Error("division by zero");
    }
    return Result.Ok(divide_toward_zero(a, b));
}
```

There is no exception to catch and no error code to forget. `E` is usually a `String` while a
program is young, and an enum once its failures are worth naming.

## `?`

Matching every fallible call is correct and quickly tedious. `?` handles the common shape:

```burxt
function halve_then_double(a: Int, b: Int) -> Result<Int, String> {
    let n: Int = divide(a, b)?;      // the value, or return the error right now
    return Result.Ok(n * 2);
}
```

`?` yields the success value, or **returns the failure from the enclosing function immediately**.
It works on `Option` the same way, in a function that answers with one:

```burxt
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

`neighbour(xs, 8)` answers `Some(2)`; `neighbour(xs, 99)` answers `None` without the second line
ever running.

The detail that makes this possible: **`?` recognises failure by the VARIANT name** — `Error`, or
`None` — never by the enum's own name. That is exactly what lets `Option` and `Result` be library
types rather than built-ins, and it means your own enum with a `None` or `Error` variant works with
`?` without asking anyone's permission.

## What is deliberately absent

**There is no `unwrap`.** A function that says "give me the value and abort if there is none" is
null with extra steps, and the crash it causes is precisely the one this type exists to prevent.
`option_or` covers the case where a default is right; `match` covers the case where it is not.
Between them there is no case left that wants a panic.

**There is no `map`.** It needs a function as a value, and a closure needs an owner for its
captured state — which is a memory question, not a syntax one. Deferred with the reason recorded,
not forgotten.

**`?` does not convert between error types.** If the callee fails with a `String` and you fail with
something else, write the `match`. Somebody has to decide what the caller's failure *means*, and
that decision does not belong to an operator.

**A payload may not be another enum.** `Option<Point>` works — a record payload is fine — but
`Option<Inner>` where `Inner` is an enum is refused, because an enum inside an enum has no finite
size without indirection. That is a memory question rather than a layout one, and it is the last
thing a variant cannot carry.

## The pattern to take away

Three of those four refusals are the same move: **when a shortcut would hide a decision, the
language asks for the decision instead.** That is what "friendly but never compromised" means
here — `?` exists because writing `match` twenty times is friction with no decision in it, and
`unwrap` does not exist because it is a decision disguised as convenience.

## Next

[Maps](11-maps.md) — a key-value table in insertion order, which is where `Option` earns its keep:
a lookup that might find nothing is the commonest reason to reach for one.

Or the running code: [`examples/absence.bx`](../../examples/absence.bx) for this page and
[`examples/generics.bx`](../../examples/generics.bx) for the one before it. Both compile, and a
test in the suite makes sure they keep compiling.

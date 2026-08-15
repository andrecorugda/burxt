---
layout: doc
title: lib/result.bx
section: reference
description: "Failure, made explicit."
---


# `lib/result.bx`

Failure, made explicit.

```burxt
use "lib/result.bx";
```

A function that can fail says so in its type. There is no exception to catch and no error code to forget, because the compiler will not let a `Result` be read without saying what happens when it is an error.

A LIBRARY file, like lib/option.bx: `Result<T, E>` is three lines of Burxt.

It also holds the §D1o error helpers, at the foot of the file: `option_ok_or`, which turns an absence into a stated reason, and the four that stop the program when there is no caller who could act on a failure. The section there argues for why they sit next to the type that exists to avoid them.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Result`](#result) | enum | Either an answer or a reason there is none. `E` is usually a String while a program is young and an enum once its failur |
| [`result_or`](#result-or) | function | The answer, or the fallback. For when a failure has an obvious substitute. |
| [`result_is_ok`](#result-is-ok) | function | — |
| [`result_is_error`](#result-is-error) | function | The other half, and it is not `!result_is_ok(r)` at the call site by accident: a condition spelled `if result_is_error(r |
| [`result_context`](#result-context) | function | `context` and `": "` in front of the failure, if there is one. An `Ok` passes through untouched. |
| [`option_ok_or`](#option-ok-or) | function | An absence, given a reason. `Option<T>` says there is nothing there; `Result<T, E>` says why, and this is the one-line c |
| [`assert_that`](#assert-that) | function | Stop unless `held`. The message says what was expected, and it should say the expectation rather than the symptom: `asse |
| [`panic`](#panic) | function | Stop, saying why. The one the other three are written in terms of. |
| [`todo`](#todo) | function | A path that is not written yet. `return todo();` type-checks anywhere an `Int` is wanted, and stops loudly the first tim |
| [`unreachable`](#unreachable) | function | A branch the program's own logic says cannot happen. Distinct from `todo()` in what it CLAIMS: `todo()` says "I have not |

## Types
{: #types}

### `Result`
{: #result}

```burxt
enum Result<T, E>
```

Either an answer or a reason there is none. `E` is usually a String while a program is young and an enum once its failures are worth naming.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L24)

## Functions
{: #functions}

### `result_or`
{: #result-or}

```burxt
function result_or<T, E>(r: Result<T, E>, fallback: T) -> T
```

The answer, or the fallback. For when a failure has an obvious substitute.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L48)

### `result_is_ok`
{: #result-is-ok}

```burxt
function result_is_ok<T, E>(r: Result<T, E>) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L55)

### `result_is_error`
{: #result-is-error}

```burxt
function result_is_error<T, E>(r: Result<T, E>) -> Bool
```

The other half, and it is not `!result_is_ok(r)` at the call site by accident: a condition spelled `if result_is_error(r)` reads the way the reader is thinking, and a `!` in front of a predicate is the single easiest thing to miss in a diff.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L65)

### `result_context`
{: #result-context}

```burxt
function result_context<T>(r: Result<T, String>, context: String) -> Result<T, String>
```

`context` and `": "` in front of the failure, if there is one. An `Ok` passes through untouched.

```burxt
 let text: String = result_context(read_config(path), "reading " + path)?;
```

**`String` errors only, and that is a real limit rather than an oversight.** A generic `E` cannot be concatenated — nothing in the language says an arbitrary `E` has any text in it — and the alternative, an interface with a `describe()` method, would make every enum-typed failure in a program implement it before this function could be called on any of them. A program whose failures have grown into an enum has outgrown this helper and should write its own `match`, which is the same advice `result_or` gives.

Two `+` for one call and no loop, so §D0's chunk rule does not apply — that rule is about building a String piece by piece in a loop, and this is a single concatenation of three pieces.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L92)

### `option_ok_or`
{: #option-ok-or}

```burxt
function option_ok_or<T, E>(o: Option<T>, why: E) -> Result<T, E>
```

An absence, given a reason. `Option<T>` says there is nothing there; `Result<T, E>` says why, and this is the one-line conversion between them.

```burxt
 let user: User = option_ok_or(find_user(id), "no user " + to_string(id))?;
```

**It lives here rather than in `lib/option.bx`** because it hands back a `Result`: a caller who can use the answer already has this file open, and `lib/option.bx` stays the module with no dependencies — every other module in this library imports it.

The reason is taken BY VALUE, like `option_or`'s fallback, so a caller building an expensive message pays for it whether or not the Option was empty. Burxt has no closures, so a lazy version would need a function value it does not have; `lib/fn.bx`'s interfaces could stand in, and the ceremony would cost more than the message.

**This was listed as blocked on A3** — `Option.None` in a free generic — for long enough that the row went stale. A3 had been working since v0.0.241, and the fixture proves it here rather than the roadmap claiming it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L118)

### `assert_that`
{: #assert-that}

```burxt
function assert_that(held: Bool, why: String) -> Int
```

Stop unless `held`. The message says what was expected, and it should say the expectation rather than the symptom: `assert_that(total == sum, "the ledger balances")` beats `"bad total"`.

**It returns `Int` rather than nothing** — Burxt has no void — and the return is ignorable, so `assert_that(...)` reads as a statement. The value is always 0; there is no other value it could have, because the alternative to returning is not returning.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L184)

### `panic`
{: #panic}

```burxt
function panic(why: String) -> Int
```

Stop, saying why. The one the other three are written in terms of.

The `return 0` after `exit(70)` is unreachable and the compiler requires it anyway: `exit` is a statement rather than an expression — `return exit(70);` is refused, because a call that ends the process has no value to give — and every path out of a function that promises an `Int` has to produce one. Left as a bare `0` rather than dressed up, because a reader should be able to see in one line that nothing here is doing anything clever.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L198)

### `todo`
{: #todo}

```burxt
function todo() -> Int
```

A path that is not written yet. `return todo();` type-checks anywhere an `Int` is wanted, and stops loudly the first time anything reaches it.

**Loud is the whole point, and it is what distinguishes this from the alternatives.** A stub that returns `0`, or an empty String, or an empty array, is a placeholder that PASSES — it will be discovered by a wrong number in a report six weeks later rather than by the first test that touches it. This one cannot be discovered late.

No message argument, because the useful information is the file and line, and the message would only ever restate the function's own name.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L214)

### `unreachable`
{: #unreachable}

```burxt
function unreachable(why: String) -> Int
```

A branch the program's own logic says cannot happen. Distinct from `todo()` in what it CLAIMS: `todo()` says "I have not written this"; `unreachable(why)` says "I have proved nothing can get here", and `why` is where that proof goes — `unreachable("the lexer only emits these four kinds")` tells whoever arrives which belief turned out to be false.

The commonest honest use is the arm after an exhaustive test that the type system cannot see is exhaustive. If a `match` covers it, use the `match`: it is checked, and this is not.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L225)


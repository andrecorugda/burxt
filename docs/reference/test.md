---
layout: doc
title: lib/test.bx
section: reference
description: "Testing Burxt, in Burxt."
---

{% raw %}

# `lib/test.bx`

Testing Burxt, in Burxt.

```burxt
use "lib/test.bx";
```

This repository tests the compiler with a Rust harness and a second one written in Burxt. A USER has neither: until now there was no way to write a test for a Burxt program at all, which is a strange thing for a language whose entire argument is that you can trust what it compiles. A language you cannot write tests in is not one anyone should ship on.

---- the shape, and why it is this one -------------------------------------------------

No registration and no `test("name", some_function)`, because **Burxt has no function values**. So a suite is a program that runs its checks in order and carries a tally:

```burxt
 region main {
     let mutable t: Tests = test_begin("invoicing");
     let a: Bool = check_money(t, "three at 19.99", line_total($19.99, 3), $59.97);
     let b: Bool = check_int(t, "empty basket", item_count(empty), 0);
     exit(test_end(t));
 }
```

`mutable t: Tests` is what makes it work, and it did not exist before v0.0.201: a function could not change anything it was passed, so a tally could not be threaded through one. And `exit(...)` did not exist before v0.0.200, so a suite could not tell a shell it failed — which means it could not fail a build. Those two are why this file is dated where it is rather than earlier.

---- what a Burxt test can do that others cannot ---------------------------------------

**It asserts a VALUE, not a range.** There is no `check_close` and there will not be one, because there is nothing to be close about: no float means no last-digit wobble, so `$59.97` is `$59.97` on every machine, every target and every run. In a language with floats a money test either compares with a tolerance — which hides the bug it was written to catch — or is flaky. Here the tolerance would be a lie about the arithmetic.

---- why the checks are per-type -------------------------------------------------------

A generic `check<T: Equatable>` is writable, and it cannot REPORT: printing a bare `T` is refused, because nothing said how wide it is or how to render it. "expected 5, got 4" is most of the value of a failing test, so the checks are named per type — the same call `array_sum_int` and `array_sum_money` make, for the same reason.

For anything else, `check_that` takes a Bool you computed and `fail_with` takes your own message. Two primitives, so the module extends without needing generics it cannot have.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Tests`](#tests) | class | The tally. A class rather than three loose variables so it can be threaded through one `mutable` parameter — which is th |
| [`test_begin`](#test-begin) | function | — |
| [`test_passed`](#test-passed) | function | A check that passed. Silent on purpose: a passing suite should print nothing but its summary, so that a failure is the o |
| [`fail_with`](#fail-with) | function | A check that failed, with a message you wrote. To STDERR, which is what stderr is for — so a suite's stdout stays the su |
| [`check_that`](#check-that) | function | A Bool you computed yourself. The escape hatch that keeps this module small: anything with no `check_` of its own can st |
| [`check_int`](#check-int) | function | — |
| [`check_money`](#check-money) | function | Money, at two places. **Exactly** — see the note at the top about why there is no tolerance. |
| [`check_money_half_even`](#check-money-half-even) | function | The same, for money that carries a ROUNDING CONTRACT — the result of anything that divided. |
| [`check_decimal7`](#check-decimal7) | function | A scale is part of a type and cannot be a type parameter, so a check exists per scale that needs one. Seven places is wh |
| [`check_string`](#check-string) | function | — |
| [`check_bool`](#check-bool) | function | — |
| [`test_end`](#test-end) | function | The summary, and the status a shell should see: 0 when everything passed, 1 otherwise. |

## Types
{: #types}

### `Tests`
{: #tests}

```burxt
class Tests
```

The tally. A class rather than three loose variables so it can be threaded through one `mutable` parameter — which is the whole reason this design is possible.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/test.bx#L53)

## Functions
{: #functions}

### `test_begin`
{: #test-begin}

```burxt
pure function test_begin(suite: String) -> Tests
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/test.bx#L59)

### `test_passed`
{: #test-passed}

```burxt
function test_passed(mutable t: Tests, what: String) -> Bool
```

A check that passed. Silent on purpose: a passing suite should print nothing but its summary, so that a failure is the only thing a reader has to look at.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/test.bx#L67)

### `fail_with`
{: #fail-with}

```burxt
function fail_with(mutable t: Tests, what: String, why: String) -> Bool
```

A check that failed, with a message you wrote. To STDERR, which is what stderr is for — so a suite's stdout stays the summary and a CI log shows failures where a CI log looks for them.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/test.bx#L74)

### `check_that`
{: #check-that}

```burxt
function check_that(mutable t: Tests, what: String, held: Bool) -> Bool
```

A Bool you computed yourself. The escape hatch that keeps this module small: anything with no `check_` of its own can still be tested, and the message says what was expected.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/test.bx#L82)

### `check_int`
{: #check-int}

```burxt
function check_int(mutable t: Tests, what: String, got: Int, want: Int) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/test.bx#L91)

### `check_money`
{: #check-money}

```burxt
function check_money(mutable t: Tests, what: String, got: Decimal<2>, want: Decimal<2>) -> Bool
```

Money, at two places. **Exactly** — see the note at the top about why there is no tolerance.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/test.bx#L99)

### `check_money_half_even`
{: #check-money-half-even}

```burxt
function check_money_half_even(mutable t: Tests, what: String, got: Decimal<2, RoundHalfEven>,
```

The same, for money that carries a ROUNDING CONTRACT — the result of anything that divided.

Two functions rather than one, and the reason is a rule worth meeting here rather than in a confusing error: **a contract may be ADDED to a value that has none, but never dropped.** So a parameter typed `Decimal<2>` cannot accept a `Decimal<2, RoundHalfEven>` — that would lose a stated intention — and a parameter typed `Decimal<2, RoundHalfEven>` would accept a plain one but then be claiming, in its own signature, that this test cares how the value rounds. It does not.

So: `check_money` for money, `check_money_half_even` for money that came out of a division. A value with a DIFFERENT contract — `RoundHalfUp` — has neither, and uses `check_that` with its own `==`, which is allowed because comparing never rounds.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/test.bx#L117)

### `check_decimal7`
{: #check-decimal7}

```burxt
function check_decimal7(mutable t: Tests, what: String, got: Decimal<7>,
```

A scale is part of a type and cannot be a type parameter, so a check exists per scale that needs one. Seven places is what `lib/vector.bx` works in; add a wrapper for any other, since `to_string` already renders every scale.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/test.bx#L128)

### `check_string`
{: #check-string}

```burxt
function check_string(mutable t: Tests, what: String, got: String, want: String) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/test.bx#L136)

### `check_bool`
{: #check-bool}

```burxt
function check_bool(mutable t: Tests, what: String, got: Bool, want: Bool) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/test.bx#L143)

### `test_end`
{: #test-end}

```burxt
function test_end(t: Tests) -> Int
```

The summary, and the status a shell should see: 0 when everything passed, 1 otherwise.

It does not exit by itself. `exit(test_end(t))` is one more word and it puts the process's ending where a reader can see it — the same reason `exit` is a statement rather than something a library can do behind your back.

The summary goes to STDOUT and the failures went to stderr, which is the Unix shape: a passing run says one line and a failing run says what went wrong on the stream built for it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/test.bx#L160)


{% endraw %}

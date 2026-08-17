---
layout: doc
title: lib/decimal.bx
section: reference
description: "The helpers a money language is judged on."
---

{% raw %}

# `lib/decimal.bx`

The helpers a money language is judged on.

```burxt
use "lib/decimal.bx";
```

Burxt's headline is that `$0.10 / 3` is a question you have to answer rather than one the machine answers wrongly for you. This file is where that promise gets cashed: **`money_split`** splits a total into parts whose sum is the total, exactly, on every input — the canonical exact-money problem, and the first function a reader comes looking for.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`decimal2_cents`](#decimal2-cents) | function | The number of pennies in `value`. `decimal2_cents($19.99)` is `1999`, and `decimal2_cents(-$0.01)` is `-1`. |
| [`decimal2_from_cents`](#decimal2-from-cents) | function | The inverse: a count of pennies as money. `decimal2_from_cents(1999)` is `$19.99`. |
| [`decimal2_is_zero`](#decimal2-is-zero) | function | Is this exactly zero? `$0.00`, and nothing else — there is no epsilon here and there will not be one, because there is n |
| [`decimal2_abs`](#decimal2-abs) | function | The distance from zero. **`requires` the value is not the most negative one**, for the reason `math_abs` states: the two |
| [`decimal2_sign`](#decimal2-sign) | function | -1, 0 or 1. No precondition and none needed: nothing is negated, so the most negative value answers -1 rather than trapp |
| [`decimal2_round_to`](#decimal2-round-to) | function | `value` rounded to `places` decimal places, still typed `Decimal<2>`. `places` is 0, 1 or 2: whole currency units, tenth |
| [`decimal2_percent_of`](#decimal2-percent-of) | function | `rate` of `amount`, rounded half-even to the penny. `decimal2_percent_of($100.00, 8.25%)` is `$8.25` exactly, with nothi |
| [`money_split`](#money-split) | function | * **`parts` larger than the penny count.** `money_split($0.02, 5)` is |
| [`decimal4_ticks`](#decimal4-ticks) | function | The count of ten-thousandths. `decimal4_ticks(8.25%)` is `825`. |
| [`decimal4_from_ticks`](#decimal4-from-ticks) | function | — |
| [`decimal4_is_zero`](#decimal4-is-zero) | function | — |
| [`decimal4_abs`](#decimal4-abs) | function | — |
| [`decimal4_sign`](#decimal4-sign) | function | — |
| [`decimal6_ticks`](#decimal6-ticks) | function | The count of millionths. |
| [`decimal6_from_ticks`](#decimal6-from-ticks) | function | — |
| [`decimal6_is_zero`](#decimal6-is-zero) | function | — |
| [`decimal7_ticks`](#decimal7-ticks) | function | The count of ten-millionths. The integer `magnitude_of_squared` searches for by hand; this is the same search named once |
| [`decimal7_from_ticks`](#decimal7-from-ticks) | function | — |
| [`decimal7_is_zero`](#decimal7-is-zero) | function | — |
| [`decimal7_abs`](#decimal7-abs) | function | — |
| [`decimal7_sign`](#decimal7-sign) | function | — |
| [`divide_round_half_even`](#divide-round-half-even) | function | `n / step`, rounded to the nearest whole, ties to the EVEN one. `divide_round_half_even(15, 10)` is 2 and `divide_round_ |

## Functions
{: #functions}

### `decimal2_cents`
{: #decimal2-cents}

```burxt
pure function decimal2_cents(value: Decimal<2>) -> Int
```

The number of pennies in `value`. `decimal2_cents($19.99)` is `1999`, and `decimal2_cents(-$0.01)` is `-1`.

Exact for every `Decimal<2>` including both extremes of the range. See the header for why this is a search rather than a cast, and why the sign is decided before the descent starts.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L83)

### `decimal2_from_cents`
{: #decimal2-from-cents}

```burxt
pure function decimal2_from_cents(cents: Int) -> Decimal<2>
```

The inverse: a count of pennies as money. `decimal2_from_cents(1999)` is `$19.99`.

The `penny * n` shape `lib/json.bx` already uses, named so the round trip has two ends a reader can see. **This is the only multiplication in the file that can trap**, and only when `cents` is the full i64 range — which is exactly when the answer does not exist.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L108)

### `decimal2_is_zero`
{: #decimal2-is-zero}

```burxt
pure function decimal2_is_zero(value: Decimal<2>) -> Bool
```

Is this exactly zero? `$0.00`, and nothing else — there is no epsilon here and there will not be one, because there is no float to wobble. That is the entire argument for this language, so a tolerance would be a lie about the arithmetic.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L118)

### `decimal2_abs`
{: #decimal2-abs}

```burxt
pure function decimal2_abs(value: Decimal<2>) -> Decimal<2>
```

The distance from zero. **`requires` the value is not the most negative one**, for the reason `math_abs` states: the two's-complement range is asymmetric, so the magnitude of the smallest `Decimal<2>` is not a `Decimal<2>`. A precondition rather than a wrong answer.

The clause compares against `decimal2_from_cents(INT_MIN)` — one multiply — rather than against `decimal2_cents(value)`, which would run the sixty-three-step search on every call to say the same thing. A contract clause is code and costs what code costs.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L130)

### `decimal2_sign`
{: #decimal2-sign}

```burxt
pure function decimal2_sign(value: Decimal<2>) -> Int
```

-1, 0 or 1. No precondition and none needed: nothing is negated, so the most negative value answers -1 rather than trapping. The `Int` return matches `math_sign`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L142)

### `decimal2_round_to`
{: #decimal2-round-to}

```burxt
pure function decimal2_round_to(value: Decimal<2>, places: Int) -> Decimal<2>
```

`value` rounded to `places` decimal places, still typed `Decimal<2>`. `places` is 0, 1 or 2: whole currency units, tenths, or the identity.

**HALF-EVEN**, which is `RoundHalfEven` — the language's own contract, the one financial reporting asks for, and the one that does not drift upward over a long ledger. To one place, `$0.05` is `$0.00` and `$0.15` is `$0.20`: both are exact ties, and the tie goes to the EVEN neighbour rather than always upward. Half-up would round both up, and a ledger full of ties rounded one way gains a penny per tie.

**Why this exists when the language has `Decimal<2, RoundHalfEven>`**: a contract governs an operation that NARROWS a scale, and there is no narrowing here — the answer stays at scale 2, it just has zeroes at the end. `$1.005` is not a `Decimal<2>` in the first place. So the language's rounding cannot express this, and the arithmetic is done on the penny count where the tie rule is four visible lines instead of a keyword.

**It also keeps the contract off the caller.** A `Decimal<2, RoundHalfEven>` cannot be passed to anything typed plain `Decimal<2>` — `array_sum_money` would refuse it — so a helper that returned one would poison every value that passed through it. Every function here answers a plain `Decimal<2>` for that reason.

**Within a penny of the top of the range, rounding UP has nowhere to go, and this traps** — `burxt runtime error: arithmetic overflow`, measured, in `tests/panic/round_to_at_the_ceiling.bx`. That is the language's own trap and it is the right answer: the rounded value does not exist as a `Decimal<2>`, so the alternatives are wrapping to a large negative number or clamping to a value nobody asked for. No `requires` guards it, because stating the condition would mean naming both the step and the leftover in the signature, and the trap already says the same thing at the same moment with a better message.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L182)

### `decimal2_percent_of`
{: #decimal2-percent-of}

```burxt
pure function decimal2_percent_of(amount: Decimal<2>, rate: Decimal<4>) -> Decimal<2>
```

`rate` of `amount`, rounded half-even to the penny. `decimal2_percent_of($100.00, 8.25%)` is `$8.25` exactly, with nothing to round.

The interesting case is half a penny. `decimal2_percent_of($0.01, 50%)` is `$0.00` and `decimal2_percent_of($0.03, 50%)` is `$0.02` — both are ties, and each goes to the even penny count rather than both going up. Half-up would answer `$0.01` and `$0.02`, which is a penny created out of nothing on every other tie.

**The multiplication is done in Decimal, not in Int, and that is deliberate.** `amount * rate` is an exact `Decimal<6>` and the language traps if that product overflows; doing it as `cents * ticks` in Int would silently need an overflow guard this function would then have to answer for. Letting the language trap keeps one rule for overflow instead of two.

A `Decimal<4>` for the rate because that is what a percent literal is: `8.25%` is `0.0825` held at scale 4, per `spec/1.0/A4.7-SIGNATURE-GRAMMAR.md`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L211)

### `money_split`
{: #money-split}

```burxt
pure function money_split(total: Decimal<2>, parts: Int) -> [Decimal<2>]
```

* **`parts` larger than the penny count.** `money_split($0.02, 5)` is

```burxt
 `[$0.01, $0.01, $0.00, $0.00, $0.00]`. Zero is a legal share; three payees getting nothing
 is the truthful answer to "split two pennies five ways".
```

* **A total of zero** gives `parts` zeroes, and they sum to zero. * **`parts` of 1** gives the total back, which the remainder handles without a special case:

```burxt
 the remainder of anything by 1 is 0.
```

* **`parts` of 0** is refused by `requires`. "Split this nought ways" has no answer, and

```burxt
 `lib/array.bx`'s `array_min` records the same call for the same reason.
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L259)

### `decimal4_ticks`
{: #decimal4-ticks}

```burxt
pure function decimal4_ticks(value: Decimal<4>) -> Int
```

The count of ten-thousandths. `decimal4_ticks(8.25%)` is `825`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L292)

### `decimal4_from_ticks`
{: #decimal4-from-ticks}

```burxt
pure function decimal4_from_ticks(ticks: Int) -> Decimal<4>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L309)

### `decimal4_is_zero`
{: #decimal4-is-zero}

```burxt
pure function decimal4_is_zero(value: Decimal<4>) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L314)

### `decimal4_abs`
{: #decimal4-abs}

```burxt
pure function decimal4_abs(value: Decimal<4>) -> Decimal<4>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L319)

### `decimal4_sign`
{: #decimal4-sign}

```burxt
pure function decimal4_sign(value: Decimal<4>) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L329)

### `decimal6_ticks`
{: #decimal6-ticks}

```burxt
pure function decimal6_ticks(value: Decimal<6>) -> Int
```

The count of millionths.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L349)

### `decimal6_from_ticks`
{: #decimal6-from-ticks}

```burxt
pure function decimal6_from_ticks(ticks: Int) -> Decimal<6>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L366)

### `decimal6_is_zero`
{: #decimal6-is-zero}

```burxt
pure function decimal6_is_zero(value: Decimal<6>) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L371)

### `decimal7_ticks`
{: #decimal7-ticks}

```burxt
pure function decimal7_ticks(value: Decimal<7>) -> Int
```

The count of ten-millionths. The integer `magnitude_of_squared` searches for by hand; this is the same search named once, and `lib/vector.bx` predates it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L382)

### `decimal7_from_ticks`
{: #decimal7-from-ticks}

```burxt
pure function decimal7_from_ticks(ticks: Int) -> Decimal<7>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L399)

### `decimal7_is_zero`
{: #decimal7-is-zero}

```burxt
pure function decimal7_is_zero(value: Decimal<7>) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L404)

### `decimal7_abs`
{: #decimal7-abs}

```burxt
pure function decimal7_abs(value: Decimal<7>) -> Decimal<7>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L409)

### `decimal7_sign`
{: #decimal7-sign}

```burxt
pure function decimal7_sign(value: Decimal<7>) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L419)

### `divide_round_half_even`
{: #divide-round-half-even}

```burxt
pure function divide_round_half_even(n: Int, step: Int) -> Int
```

`n / step`, rounded to the nearest whole, ties to the EVEN one. `divide_round_half_even(15, 10)` is 2 and `divide_round_half_even(25, 10)` is also 2 — both ties, and both go to the even neighbour rather than both going up.

Written once, in Int, and shared by every scale — which is the payoff of converting to the unscaled count first: the rounding rule exists in exactly one place, so there is one thing to read and one thing to be wrong.

**It answers the QUOTIENT, not the rounded multiple**, because both callers want the quotient and only one of them wants it multiplied back up. Returning the multiple was the first version and it made `decimal2_percent_of($100.00, 8.25%)` answer `$82,500.00` — a factor of 10,000, which is exactly the kind of mistake a fixture catches and a reading does not.

`divide_floor` rather than truncation here, and unlike `money_split` that is the right choice: flooring makes the leftover non-negative for negative `n` too, so "is this a tie" is one comparison instead of two. -5 with step 10 gives quotient -1 and leftover 5, a tie, and the even neighbour is 0 — which is `decimal2_round_to(-$0.05, 1)` being `$0.00`, the mirror of the positive case. Half-even is symmetric about zero, and `tests/pass/decimal_helpers.bx` checks that over every penny in a range rather than on the four examples above.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/decimal.bx#L453)


{% endraw %}

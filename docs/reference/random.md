---
layout: doc
title: lib/random.bx
section: reference
description: "A SEEDED generator, and the name says seeded."
---


# `lib/random.bx`

A SEEDED generator, and the name says seeded.

```burxt
use "lib/random.bx";
```

```burxt
 let mutable rng: Random = random_from(20260802);
 print(rng.next_below(6) + 1);            // a die
 let moved: Int = random_shuffle(rng, deck);
```

---- the name is the design decision, not a preference ---------------------------------------

There is `random_from(seed)` and there is no `random()`. Every value this file produces is a pure function of the seed, so the same seed replays the same run — which is exactly right for a test, a shuffle, a sample or a simulation, and **exactly wrong for a key, a token, a password or a nonce**. A bare `random()` invites the second use by looking like the thing a caller reaches for when they want unpredictability, and the mistake is silent: the program works, the tests pass, and the secret is guessable by anyone who can guess a timestamp.

So the seed is in the constructor's name and cannot be omitted. `lib/time.bx` makes the same call about units (`duration_hours`, never `duration(n, unit)`): what a reviewer must know to judge the call is in the name they are reading, not in a manual.

**OS entropy is not here, and that is on purpose.** `tests/pass/os_random_bytes.bx` already reaches `getentropy` through a `CPointer`, and it belongs in `lib/os.bx` beside the other syscalls when it is promoted rather than in the file whose whole subject is reproducibility. A second constructor in *this* file — `random_from_entropy()`, say — would sit one line below `random_from` in every listing, in the same table in `lib/README.md`, returning the same `Random` type, and a reader would have to know which of two adjacent names is safe for a key. When a CSPRNG lands it gets its own type and its own file, so the two cannot be confused by eye. Nothing here should ever be reachable from a `Csprng`.

---- the algorithm: xorshift64, and why THIS one ---------------------------------------------

Marsaglia's xorshift64 with the (13, 7, 17) triple:

```burxt
 x ^= x << 13;  x ^= x >>> 7;  x ^= x << 17;
```

Five operations, all of them `bit_xor` and shifts, and that is the reason it was chosen over PCG and over xoshiro256**: **it needs no addition and no multiplication, so it cannot trap.** Burxt's `+` and `*` refuse to overflow — which is the language working as designed — and every other generator worth having is built on arithmetic that overflows on purpose. Those are writable here (`lib/math.bx`'s `math_wrapping_mul` exists and is used two functions down), but a generator whose core loop is 64 half-adder passes per output is the wrong core loop. xorshift asks the language for nothing it does not want to give.

Its period is 2^64 - 1, over every state except zero, and the state is one `Int`.

**What is honestly wrong with it**, because a generator's weaknesses belong next to it and not in a paper somebody has to find: xorshift64 is F2-linear, so every output bit is a fixed exclusive-or of state bits. That makes it fail the linear-complexity and matrix-rank tests in TestU01's BigCrush — a test suite specifically able to see the linearity. It does not make it fail the tests a shuffle depends on: equidistribution, serial correlation and runs are fine, and `tests/pass/random_library.bx` measures the ones that matter here rather than asserting it. If you need a generator no adversary can predict, you need a CSPRNG, and see above.

---- zero, and the warm-up -------------------------------------------------------------------

Zero is a fixed point: xorshift on all-zero bits is all-zero bits, forever. `random_from(0)` is the seed a caller is most likely to write first, so it is remapped rather than refused — see `random_from`.

A small seed also starts *poorly*, which is a separate defect and easy to miss. From seed 1 the first raw output is 1082269761 — thirty bits, in a sixty-four-bit range, and a caller drawing `next_below(1000000)` twice would see two small numbers and conclude nothing was wrong. So the constructor discards `RANDOM_WARMUP` outputs before handing the generator back. It costs five operations each, once, and it is the difference between a generator that is right and one that looks right for the first few draws — which is the failure this file is most likely to have.

---- no modulo, and therefore no modulo bias -------------------------------------------------

`next_below` and `next_between` both go through one range routine, and it uses **masked rejection** rather than `remainder`. `next % bound` is biased whenever `bound` does not divide 2^64 — the low `2^64 mod bound` values come up one time in 2^64 more often than the rest — and while that is invisible for a die it is not invisible for the sampling this file will be used for. The mask form has no bias at all, needs no division, and costs one extra draw about half the time in the worst case.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Random`](#random) | class | A seeded generator. One `Int` of state, so it is cheap to hold and cheap to pass; `mutable` wherever it is used, because |
| [`random_from`](#random-from) | function | A generator from a seed. **The only way to make one**, and the seed is not optional. |
| [`random_unsigned_at_most`](#random-unsigned-at-most) | function | `a <= b`, reading both as UNSIGNED sixty-four-bit values. |
| [`random_mask_for`](#random-mask-for) | function | The smallest `2^k - 1` that is at least `value`, read as unsigned — every bit below the highest set one, filled in. The  |
| [`random_shuffle`](#random-shuffle) | function | Rearrange `xs` in place, every ordering equally likely. |
| [`random_choice`](#random-choice) | function | One element of `xs`, uniformly, or `None` when there is nothing to choose from. |
| [`next`](#next) | method on `Random` | The raw generator: all sixty-four bits, uniform over every value but the one it can never produce, which is the state it |
| [`next_between`](#next-between) | method on `Random` | A value from `low` to `high`, **both ends included**. |
| [`next_below`](#next-below) | method on `Random` | A value from 0 up to but **not including** `bound`. The form an array index wants: `next_below(len(xs))` is always a leg |

## Types
{: #types}

### `Random`
{: #random}

```burxt
class Random { state: Int }
```

A seeded generator. One `Int` of state, so it is cheap to hold and cheap to pass; `mutable` wherever it is used, because drawing a number is a change and the signature says so.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/random.bx#L94)

## Functions
{: #functions}

### `random_from`
{: #random-from}

```burxt
function random_from(seed: Int) -> Random
```

A generator from a seed. **The only way to make one**, and the seed is not optional.

Every seed is accepted, including 0 and including a negative one — a caller seeding from a row id, a timestamp or a hash should not have to know which values this file dislikes. Zero is the one value that would break the algorithm (it is xorshift's fixed point), so it becomes `RANDOM_GOLDEN` instead. Two seeds therefore share a sequence — 0 and `RANDOM_GOLDEN` — and that is the whole cost of accepting every Int.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/random.bx#L103)

### `random_unsigned_at_most`
{: #random-unsigned-at-most}

```burxt
pure function random_unsigned_at_most(a: Int, b: Int) -> Bool
```

`a <= b`, reading both as UNSIGNED sixty-four-bit values.

Needed because the distance between two Ints is genuinely unsigned: from `INT_MIN` to `INT_MAX` is 2^64 - 1, which no `Int` can hold as a positive number. Flipping the top bit of both operands turns the unsigned order into the signed one, exactly, with no branch and no wider type.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/random.bx#L142)

### `random_mask_for`
{: #random-mask-for}

```burxt
pure function random_mask_for(value: Int) -> Int
```

The smallest `2^k - 1` that is at least `value`, read as unsigned — every bit below the highest set one, filled in. The standard bit-smear: or the value with itself shifted down by 1, 2, 4, 8, 16 and 32, and every gap closes.

`shift_right_zeros` and not `shift_right_sign`: on a value with the top bit set the sign-copying shift would be filling with ones it is trying to detect, and the answer would be right by accident. With zeros, a top-bit value smears to all ones, which is the correct mask for it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/random.bx#L153)

### `random_shuffle`
{: #random-shuffle}

```burxt
function random_shuffle<T>(mutable rng: Random, mutable xs: [T]) -> Int
```

Rearrange `xs` in place, every ordering equally likely.

**This changes YOUR array** — `mutable xs: [T]`, the same signature `lib/array.bx` uses to say it, and assigning an array does not copy it. `array_copy` first if you need the original.

Fisher-Yates walking DOWNWARDS, which is the only version of this algorithm that is right. Walking upwards with an index drawn from the whole array — `j = next_below(len(xs))` for every `i` — is the famous wrong shuffle: it has n^n equally likely execution paths over n! orderings, n! never divides n^n for n > 2, so some orderings come up more often than others. It looks correct, it passes a test that only checks the elements are still all there, and it is biased. The correct draw is from `0..i` inclusive, which is `next_below(i + 1)`.

An empty array and a one-element array are both no-ops that **draw nothing**: the loop starts at `len(xs) - 1` and does not run. Deliberate, and worth knowing if you are counting draws — a shuffle of one element consumes no randomness, so seeding, shuffling a singleton and then drawing gives the same value as seeding and drawing.

Answers the length, like `array_reverse` and `array_sort`, so the call has something to bind.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/random.bx#L253)

### `random_choice`
{: #random-choice}

```burxt
function random_choice<T>(mutable rng: Random, xs: [T]) -> Option<T>
```

One element of `xs`, uniformly, or `None` when there is nothing to choose from.

**The empty array answers `Option.None`**, and this is the case where that is right where `next_below(0)`'s contract failure was right. The difference is that "choose from nothing" has a truthful answer and "a number below zero" does not: there is no element, so absence *is* the result, and `Option` is the type this language has for saying so. A caller who wants the crash can `match` and panic; a caller who did not think about empty is made to by the `match`.

`xs` is not `mutable`: choosing reads. `rng` is, because drawing writes.

**Draws nothing on the empty array**, same as `random_shuffle` — the `None` is returned before the generator is touched, so a failed choice does not advance the sequence.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/random.bx#L277)

## Methods
{: #methods}

### `next`
{: #next}

```burxt
function (mutable self: Random) next() -> Int
```

The raw generator: all sixty-four bits, uniform over every value but the one it can never produce, which is the state it can never be in.

Public, and for the same reason `string_join_chunks` is: a caller who needs bits rather than a range — a hash seed, a random `Bool` from the low bit, a byte at a time — would otherwise re-derive xorshift beside this file, and the copy would be the one without the warm-up.

Not `pure`, and could not be: it changes `self`. Nothing in this file can appear in a contract clause, which is correct — a `requires` that drew a random number would answer differently every time it ran.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/random.bx#L126)

### `next_between`
{: #next-between}

```burxt
function (mutable self: Random) next_between(low: Int, high: Int) -> Int
```

A value from `low` to `high`, **both ends included**.

Inclusive, and the two reasons are worth stating because half the libraries in the world choose the other one:

1. With an excluded upper end there is no way to ask for `INT_MAX`, because `INT_MAX + 1` does

```burxt
  not exist. A range that cannot name its own bound is a trap, and it is a trap at exactly
  the boundary a test is least likely to cover.
```

2. `next_between(1, 6)` then means what a reader thinks it means. `next_below` is the

```burxt
  exclusive form, it is named for the word "below", and one of the two spellings should be
  each.
```

**`low == high` answers `low`, and draws nothing.** Not an error: a range of one value has an answer, and a caller computing its bounds from data should not have to special-case the day the data has one row. It costs no randomness, so a loop that narrows a range to a point stops consuming the sequence rather than churning it.

**`low > high` is REFUSED**, by contract. An inverted range has no values in it, so there is nothing truthful to answer; and in practice it means the two arguments were passed the wrong way round, which silently swapping them would hide forever.

The distance is computed with `math_wrapping_sub` and applied with `math_wrapping_add`, and the wrap is not a shortcut — it is the arithmetic being correct. `high - low` genuinely does not fit in an `Int` for a wide range (`INT_MIN` to `INT_MAX` is 2^64 - 1), so the distance is held as an unsigned bit pattern and compared with `random_unsigned_at_most`. This is the one place in the file where a value is not a number; every operation on `spread` treats it as bits.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/random.bx#L190)

### `next_below`
{: #next-below}

```burxt
function (mutable self: Random) next_below(bound: Int) -> Int
```

A value from 0 up to but **not including** `bound`. The form an array index wants: `next_below(len(xs))` is always a legal subscript.

**`bound == 0` is REFUSED**, by contract, rather than answering 0. There is no value below 0 that is at least 0, so every possible answer is a lie, and 0 is the most damaging of the lies available because it is a valid-looking index into the empty array the caller is holding. The call almost always reads `next_below(len(xs))` on a collection that turned out to be empty, and a contract failure names that at the point it happens instead of one frame later. See `random_choice`, which is the same question asked in a form that *does* have an honest answer — `Option.None` — and which is what to reach for when empty is expected.

`bound == 1` answers 0 every time, draws nothing, and is not a special case in the code: it falls out of `next_between(0, 0)`.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/random.bx#L222)


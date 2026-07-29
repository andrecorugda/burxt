# 2. Numbers and money

This is the page the language exists for.

## The problem

```js
> 0.1 + 0.2
0.30000000000000004
> 19.99 * 3
59.96999999999999
```

Binary floating point cannot represent `0.1`, or `19.99`, or most prices. Every language
with a `float` default has this, and every finance codebase written in one has a layer of
discipline on top: use cents, use a Decimal library, round at the boundary, review
carefully. Discipline is what fails at 2am.

## The answer

A `Decimal<S>` is an integer scaled by `10^S`. `19.99` is the integer `1999` with a scale of
2 — exact, and exact for the same reason `199 + 1` is exact.

```burxt
let price: Decimal<2> = 19.99;      // or $19.99 — the same value
let quantity:   Int        = 3;
let total: Decimal<2> = price * quantity;
print(total);                        // 59.97, exactly
```

The scale lives **in the type**. `Decimal<2>` and `Decimal<4>` are different types, and the
compiler will not quietly turn one into the other.

## Literals

| You write | It means | Its type |
|---|---|---|
| `19.99` | 19.99 | `Decimal<2>` |
| `$19.99` | 19.99 | `Decimal<2>` — the `$` is documentation, not a currency |
| `8.25%` | 0.0825 | **`Decimal<4>`** — a percent is two places finer than the number in it |
| `42` | 42 | `Int` (64-bit, and it traps on overflow) |

`8.25%` being a `Decimal<4>` surprises people once. It is arithmetic: 8.25 percent *is*
0.0825, and 0.0825 needs four decimal places.

A literal takes the scale of its context when that loses nothing: `let x: Decimal<2> = $5;`
is `5.00`. When it would lose something, it is refused rather than rounded.

## Addition: scales must match

```burxt
let price: Decimal<2> = $19.99;
let rate:  Decimal<4> = 8.25%;
let sum:   Decimal<2> = price + rate;
```

```
error: cannot + Decimal<2> and Decimal<4>: scales must match. Burxt does not
       silently rescale money.
```

Addition combines *like quantities*. A price and a rate are not like quantities, and the
compiler will not pick a scale on your behalf — picking one is exactly the silent decision
this language exists to prevent.

## Multiplication: mixed scales, and a contract where it rounds

Multiplication is different, because multiplying a quantity by a *rate* is the normal thing
to do and their scales differ by nature:

```burxt
let price: Decimal<2> = $19.99;
let rate:  Decimal<4> = 8.25%;
let tax:   Decimal<2, RoundHalfEven> = price * rate;   // 1.65
```

The exact product of a 2-place and a 4-place number has **6** places. Landing it on 2 means
rounding, so the type must say how: `Decimal<2, RoundHalfEven>` is a *rounding contract*.
Without one:

```
error: this multiplication of Decimal<2> by Decimal<4> has an exact product with 6
       decimal places, and reaching Decimal<2> means rounding it. Say how —
       Decimal<2, RoundHalfEven> — or take the exact answer with Decimal<6>.
```

**Or take the exact answer, which needs no contract at all**, because nothing rounds:

```burxt
let exact = price * rate;             // Decimal<6> — 1.649175, all of it
let same:  Decimal<6> = price * rate; // the same thing, written down
```

That is the rule in one line: **a contract is required exactly where a value narrows.** Its
presence in a program is therefore information — somebody made a decision here — which is
what it is for. (Until v0.0.91 a contract was demanded on every `Decimal * Decimal`, exact or
not. That taught readers it was ceremony, and ceremony is what readers learn to skip.)

One convenience worth knowing: when both operands have the **same type including a contract**,
that contract lands the product at their scale, so the context does not have to repeat it:

```burxt
let a: Decimal<2, RoundHalfEven> = 1.05;
let b: Decimal<2, RoundHalfEven> = 0.10;
print(a * b);                         // 0.11 — money times money answers in money
```

Two contracts exist: `RoundHalfEven` (banker's rounding — the default in most financial
regulation, because it does not bias upward over many transactions) and `RoundHalfUp`.

**Division always needs a contract**, even when the scales match, because a quotient can
fall between two representable values: `1.00 / 3` has no exact answer at two places.

## Where the contract goes

Write it **where the rounding happens**, not where the money enters:

```burxt
let price: Decimal<2> = $19.99;                       // no contract needed
let subtotal: Decimal<2> = price * 3;                 // exact, none needed
let rate: Decimal<4> = 8.25%;
let tax: Decimal<2, RoundHalfEven> = subtotal * rate; // ← here, where it rounds
let total: Decimal<2, RoundHalfEven> = subtotal + tax;
```

Two rules make that work, and both are narrower than they used to be (v0.0.86):

**A contract may be added where a value has none.** `Decimal<2>` and
`Decimal<2, RoundHalfEven>` hold the *same integer* — a contract does not reinterpret the
value, it constrains what future operations may do to it. So it can arrive at the binding
that needs it.

**Addition and subtraction need matching scales, not matching contracts.** They never round,
so a contract on one side and none on the other leaves exactly one answer to "if this ever
rounds, which way", and the result carries it.

What is still refused, and why:

| | |
|---|---|
| `Decimal<2> + Decimal<4>` | Scales must match. This is the rule that protects money |
| `Decimal<2,HalfEven> + Decimal<2,HalfUp>` | Two different contracts — picking one would be a decision nobody wrote down |
| `let plain: Decimal<2> = contracted;` | **Dropping** a contract loses a declared intention |

Before v0.0.86 both relaxations were errors, so a contract had to be declared at the point
money entered the program — and the fix for an error was never where the error was. It cost
three attempts on a seven-line invoice, which is a fair test of a rule.

## Integers

`Int` is a signed 64-bit integer, and **it traps rather than wrapping**:

```
burxt runtime error: arithmetic overflow — the exact result no longer fits in the
value range
```

Products are computed in 128 bits internally, so the error means the *result* does not fit,
not that an intermediate step did.

`/` on two Ints is a compile error, because one operator cannot say which way to round:

```
error: `/` on two Ints would have to round, and one operator cannot say which way —
       use divide_floor, divide_toward_zero or remainder
```

`divide_floor(-7, 2)` is `-4`. `divide_toward_zero(-7, 2)` is `-3`. They differ, they differ silently
in most languages, and here you say which one you meant.

## Next

[Types](03-types.md) — records, enums, traits, and why there is no inheritance.

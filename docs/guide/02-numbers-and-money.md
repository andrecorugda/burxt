---
title: Numbers and money
description: Money is coins you count, not water you pour. A Decimal is an integer and a scale, both in the type.
---

# 2. Numbers and money

## What this is for
{: #what-this-is-for}

Nobody writes a money bug on purpose. It arrives like this.

```js
> 19.99 * 3
59.96999999999999
```

The screen shows `59.97`, because whatever formats it rounds to two places. The ledger stores
`59.96999999999999`. A month later reconciliation is off by a cent and three people spend an
afternoon on it.

That one is famous enough that most people guard against it. Here is the version that gets through
review, which is the one this page is really about:

```
subtotal = 59.97          # two decimal places
rate     = 0.0825         # a tax rate
tax      = subtotal * rate       # 4.947525 — six decimal places
store(tax)                       # the column holds two
```

Nothing looks wrong. There is no float error, no overflow, no exception. The product genuinely has
six decimal places and the column genuinely holds two, and *something* has to give — so a language
that stores it anyway has silently decided how to round on your behalf. Do it a hundred thousand
times and the difference is a number an auditor will eventually ask about.

**A silent wrong answer is the worst outcome a program can produce.** Worse than a crash, which you
find on Tuesday. Far worse than a refusal, which you find in the compiler.

## Think of coins in a jar, not water in a jug
{: #think-of-coins-in-a-jar-not-water-in-a-jug}

Water is the wrong shape for money. Pour it between glasses and a little stays behind every time —
not because you were careless, but because that is what pouring *is*. A float is water.

Coins are countable. Move 1999 pennies from one hand to the other and you have 1999 pennies, because
there was never anything smaller than a penny to lose.

<figure>
<svg viewBox="0 0 680 300" role="img" aria-label="Money as coins you count against money as water you pour: the coins survive being moved and the water loses drops; and two jars with different labels cannot be added" style="max-width:100%;height:auto;">
  <style>
    .jar   { fill: #ffffff; stroke: #1d1d1f; stroke-width: 2; }
    .coin  { fill: #ffffff; stroke: #0f6f3c; stroke-width: 1.6; }
    .coinf { fill: #0f6f3c; opacity: .10; }
    .water { fill: #0071e3; opacity: .18; }
    .lip   { fill: none; stroke: #1d1d1f; stroke-width: 2; stroke-linecap: round; }
    .drop  { fill: #c8102e; }
    .hair  { fill: none; stroke: #d2d2d7; stroke-width: 1; }
    .no    { fill: none; stroke: #c8102e; stroke-width: 2; }
    .arrow { fill: none; stroke: #1d1d1f; stroke-width: 1.6; marker-end: url(#m2); }
    .h     { font: 600 13.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .t     { font: 12.5px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #1d1d1f; }
    .sm    { font: 11.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .red   { font: 600 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #c8102e; }
    .lbl   { font: 600 11.5px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #0f6f3c; }
  </style>
  <defs>
    <marker id="m2" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/>
    </marker>
  </defs>

  <!-- ---- left: coins survive being moved ------------------------------------------------ -->
  <text class="h" x="8" y="18">Coins you count</text>

  <path class="jar" d="M20 40 h96 v16 a8 8 0 0 1 -8 8 h-80 a8 8 0 0 1 -8 -8 z"/>
  <path class="jar" d="M26 64 h84 v74 a10 10 0 0 1 -10 10 h-64 a10 10 0 0 1 -10 -10 z"/>
  <rect class="coinf" x="28" y="96" width="80" height="50" rx="8"/>
  <circle class="coin" cx="48" cy="118" r="10"/>
  <circle class="coin" cx="70" cy="118" r="10"/>
  <circle class="coin" cx="92" cy="118" r="10"/>
  <circle class="coin" cx="59" cy="138" r="10"/>
  <circle class="coin" cx="81" cy="138" r="10"/>
  <text class="t"   x="20" y="170">1999 pennies</text>
  <text class="lbl" x="20" y="187">label: 2</text>

  <text class="sm" x="132" y="106">move</text>
  <text class="sm" x="132" y="121">them</text>
  <path class="arrow" d="M130 130 h44"/>

  <path class="jar" d="M182 40 h96 v16 a8 8 0 0 1 -8 8 h-80 a8 8 0 0 1 -8 -8 z"/>
  <path class="jar" d="M188 64 h84 v74 a10 10 0 0 1 -10 10 h-64 a10 10 0 0 1 -10 -10 z"/>
  <rect class="coinf" x="190" y="96" width="80" height="50" rx="8"/>
  <circle class="coin" cx="210" cy="118" r="10"/>
  <circle class="coin" cx="232" cy="118" r="10"/>
  <circle class="coin" cx="254" cy="118" r="10"/>
  <circle class="coin" cx="221" cy="138" r="10"/>
  <circle class="coin" cx="243" cy="138" r="10"/>
  <text class="t"   x="182" y="170">1999 pennies</text>
  <text class="lbl" x="182" y="187">exactly 19.99</text>

  <line class="hair" x1="316" y1="8" x2="316" y2="196"/>

  <!-- ---- right: water loses drops ------------------------------------------------------- -->
  <text class="h" x="348" y="18">Water you pour</text>

  <path class="jar" d="M352 46 h58 v88 a12 12 0 0 1 -12 12 h-34 a12 12 0 0 1 -12 -12 z"/>
  <path class="water" d="M354 80 h54 v54 a12 12 0 0 1 -12 12 h-30 a12 12 0 0 1 -12 -12 z"/>
  <path class="lip" d="M410 58 q16 6 12 22"/>
  <text class="t" x="352" y="170">19.99</text>

  <path class="water" d="M430 78 q13 20 18 40 l-9 2 q-5 -20 -17 -38 z"/>
  <circle class="drop" cx="456" cy="98" r="3"/>
  <circle class="drop" cx="466" cy="116" r="2.5"/>
  <circle class="drop" cx="460" cy="132" r="2"/>
  <text class="red" x="476" y="104">drops</text>
  <text class="red" x="476" y="121">lost</text>

  <path class="jar" d="M408 118 h54 v28 a10 10 0 0 1 -10 10 h-34 a10 10 0 0 1 -10 -10 z"/>
  <path class="water" d="M410 128 h50 v18 a10 10 0 0 1 -10 10 h-30 a10 10 0 0 1 -10 -10 z"/>
  <text class="t" x="348" y="187">19.98999999999</text>

  <line class="hair" x1="8" y1="212" x2="672" y2="212"/>

  <!-- ---- bottom: two labels do not add -------------------------------------------------- -->
  <text class="h" x="8" y="238">Two labels do not add</text>

  <rect class="jar" x="20" y="252" width="92" height="40" rx="8"/>
  <rect class="coinf" x="24" y="256" width="84" height="32" rx="6"/>
  <text class="t"   x="30" y="270">a price</text>
  <text class="lbl" x="30" y="285">label: 2</text>

  <text class="t" x="126" y="277">+</text>

  <rect class="jar" x="148" y="252" width="92" height="40" rx="8"/>
  <rect class="coinf" x="152" y="256" width="84" height="32" rx="6"/>
  <text class="t"   x="158" y="270">a tax rate</text>
  <text class="lbl" x="158" y="285">label: 4</text>

  <g class="no">
    <circle cx="272" cy="272" r="14"/>
    <line x1="262" y1="262" x2="282" y2="282"/>
  </g>

  <text class="red" x="300" y="268">A penny and a ten-thousandth</text>
  <text class="red" x="300" y="285">are not the same coin.</text>
</svg>
<figcaption>A <code>Decimal&lt;2&gt;</code> is 1999 and a label saying <em>how many of these make one</em>. Nothing
divides, so nothing is lost — and adding two different labels has no answer to give, which is why
Burxt refuses it where a float language quietly picks one.</figcaption>
</figure>

That is the whole idea. There is no `19.99` anywhere in a Burxt program's memory: there is the whole
number **1999** and a scale, and the scale lives in the type.

## A step closer
{: #a-step-closer}

`Decimal<2>` and `Decimal<4>` are **different types**, the way `Int` and `Bool` are different types.
The compiler will never quietly turn one into the other, because there is no answer to turn it into:

<div class="tablewrap" markdown="1">

| You write | Stored as | The label | So it means |
|---|---|---|---|
| `19.99` | `1999` | 2 | 1999 / 10² = 19.99 |
| `$19.99` | `1999` | 2 | the same value — the `$` is documentation, not a currency |
| `8.25%` | `825` | 4 | 825 / 10⁴ = 0.0825 |
| `42` | `42` | — | `Int`, signed 64-bit, and it traps on overflow |

</div>

`8.25%` being a `Decimal<4>` surprises everyone exactly once. It is arithmetic, not a rule: 8.25
percent *is* 0.0825, and 0.0825 needs four places.

Add `1999` and `825` and you get `2824`. Is that 28.24 or 0.2824? **There is no answer** — and that
is not a limitation of the compiler, it is a fact about the question. Every language that returns a
number here has picked one on your behalf without writing it down.

A literal takes the scale of its context when that loses nothing: `let x: Decimal<2> = $5;` is
`5.00`. When it would lose something, it is refused rather than rounded.

## In code
{: #in-code}

**Adding needs matching labels.**

```burxt
function total(price: Decimal<2>, rate: Decimal<4>) -> Decimal<2> {
    return price + rate;
}
```

```
error: cannot + Decimal<2> and Decimal<4>: scales must match. Burxt does not
       silently rescale money.
```

**Multiplying is the opposite case.** Multiplying an amount by a rate is the normal thing to do, and
their scales differ *by definition*. Two places times four places is an exact product with **six**,
so landing it on two throws four digits away — and the type has to say which way:

```burxt
let price: Decimal<2> = $19.99;
let rate:  Decimal<4> = 8.25%;
let tax:   Decimal<2, RoundHalfEven> = price * rate;   // 1.65
```

Leave the contract off and you are told, with both numbers and both ways out:

```
error: this multiplication of Decimal<2> by Decimal<4> has an exact product with 6
       decimal places, and reaching Decimal<2> means rounding it. Say how —
       Decimal<2, RoundHalfEven> — or take the exact answer with Decimal<6>.
```

**Or keep every digit**, which needs no contract at all, because nothing rounds:

```burxt
let price: Decimal<2> = $19.99;
let rate:  Decimal<4> = 8.25%;
let exact = price * rate;             // Decimal<6> — 1.649175, all of it
let same: Decimal<6> = price * rate;  // the same thing, written down
```

The two contracts are `RoundHalfEven` — banker's rounding, the default in most financial regulation
because it does not bias upward across many transactions — and `RoundHalfUp`.

**Division always needs one**, even when the scales already match, because a quotient can land
between two representable values: `1.00 / 3` has no exact answer at two places, and no scale you
could pick would give it one.

### Where the contract goes
{: #where-the-contract-goes}

Where the rounding happens. Not where the money enters:

```burxt
let price: Decimal<2> = $19.99;                       // no contract needed
let subtotal: Decimal<2> = price * 3;                 // exact, none needed
let rate: Decimal<4> = 8.25%;
let tax: Decimal<2, RoundHalfEven> = subtotal * rate; // ← here, where it rounds
let total: Decimal<2, RoundHalfEven> = subtotal + tax;
```

Two things make that read the way you would hope.

**A contract may be added where a value has none.** `Decimal<2>` and `Decimal<2, RoundHalfEven>`
hold the *same integer*. A contract does not reinterpret a value; it constrains what future
operations are allowed to do to it. So it can arrive at the binding that needs it.

**Adding and subtracting need matching scales, not matching contracts.** They never round, so one
side carrying a rule and the other not leaves exactly one answer to *"if this ever rounds, which
way"* — and the result carries it.

### A rounding rule travels
{: #a-rounding-rule-travels}

Once a value carries `RoundHalfEven`, so does every signature it flows through:

```burxt
function line_tax(subtotal: Decimal<2>, rate: Decimal<4>) -> Decimal<2, RoundHalfEven> {
    return subtotal * rate;
}
```

That looks like a cost and is the entire mechanism. A reviewer reading the *caller* can see how this
rounds without opening the file it is defined in — and an agent cannot change the rounding of a total
without changing a declaration, which is a thing
[`burxt review`](12-tools-and-agents.md) can find.

## Why it is built this way
{: #why-it-is-built-this-way}

Three properties fall out of "an integer and a scale, both in the type", and each one is worth the
strictness on its own.

**A wrong answer cannot be plausible.** The failure mode this replaces is not a crash; it is a number
that looks right. Refusing at compile time moves the discovery from an auditor's question to a
message on your screen, and those are not the same afternoon.

**It is faster, not slower.** Scaled-integer arithmetic is integer arithmetic. There is no decimal
library, no allocation, and no floating-point unit involved — products are computed in 128 bits
internally so the check is on the *answer* rather than on an intermediate step.

**The rounding is in the signature, so a reviewer can see it.** This is the part that connects to the
rest of the language. `Decimal<2, RoundHalfEven>` in a return type means somebody decided, here, how
this total rounds — and you learn it from the declaration without reading a body. A rounding rule
hidden in a function body is a rule that can be changed in a diff nobody reads.

## What it costs
{: #what-it-costs}

Honestly: some annotations you would not have written in Python.

**You will write `Decimal<2, RoundHalfEven>` where another language wrote nothing.** That is the whole
bill. It is paid exactly where a value narrows, which is also exactly where the decision is — but it
is still typing you did not do before.

**`8.25%` is a `Decimal<4>` and that surprises everybody once.** It is correct and it is still a
surprise.

**A scale is fixed at 18 places.** Beyond that there is no type to hold it.

**A `Decimal` has no display form you can define.** `to_string` handles `Int`, `Bool` and `Decimal`,
and a class of yours cannot have one — there is no interface for it yet. That is a gap rather than a
decision, and it is recorded as one in the
[reference]({{ site.baseurl }}/reference/builtins.html#to-string).

**Nothing here helps at the boundary unless you use it.** Guarding the arithmetic and then handing
the value to a C function taking a `double` guards nothing. `as scaled` is how a `Decimal` crosses to
C without becoming a float — see [the C boundary](07-ffi.md).

## When you reach for it
{: #when-you-reach-for-it}

<div class="tablewrap" markdown="1">

| You are writing | Reach for |
|---|---|
| a price, a total, a balance, a fee | `Decimal<2>` |
| a tax rate, an interest rate, a discount | `Decimal<4>`, usually written `8.25%` |
| a subtotal that will be multiplied by a rate later | `Decimal<2>` — add the contract where it narrows, not here |
| the result of amount × rate, stored in a money column | `Decimal<2, RoundHalfEven>` |
| the result of amount × rate, kept for a later step | `Decimal<6>` — exact, no contract, nothing rounds |
| a count, an index, a quantity | `Int` |
| anything a model or a form handed you | `String` until you parse it — see [absence and failure](10-absence-and-failure.md) |

</div>

The rule of thumb: **narrow once, as late as you can.** Every rounding is a decision, and a program
with one rounding in it is a program with one decision to review.

## Examples
{: #examples}

A till line, all the way through. Every number below came from running this program.

```burxt
let price:    Decimal<2> = $19.99;
let quantity: Int        = 3;
let subtotal: Decimal<2> = price * quantity;

let rate:  Decimal<4>                = 8.25%;
let tax:   Decimal<2, RoundHalfEven> = subtotal * rate;
let total: Decimal<2, RoundHalfEven> = subtotal + tax;

print(subtotal);
print(tax);
print(total);
```

```
59.97
4.95
64.92
```

`59.97`, not `59.96999999999999`. And `4.95` is a rounding somebody named: the exact product is
`4.947525`, and `RoundHalfEven` in the type is where the decision was written down.

**Exact, or narrowed — you choose, and the type says which.**

```burxt
let price: Decimal<2> = $19.99;
let rate:  Decimal<4> = 8.25%;

let exact:   Decimal<6>                = price * rate;
let rounded: Decimal<2, RoundHalfEven> = price * rate;

print(exact);
print(rounded);
```

```
1.649175
1.65
```

**And an integer that will not wrap.** `Int` is signed 64-bit; push a result past what it holds and
the program stops rather than continuing with a negative number:

```burxt
let big: Int = 9223372036854775807;
print(big + 1);
```

```
burxt runtime error: arithmetic overflow — the exact result no longer fits in the value range
```

In C, C#, Java and Go that same expression quietly becomes a negative number.

`/` on two `Int`s is a compile error for the same reason division of money needs a contract — one
operator cannot say which way to round. `divide_floor(-7, 2)` is `-4`; `divide_toward_zero(-7, 2)`
is `-3`. They differ, they differ *silently* in most languages, and here you say which you meant.

## Next
{: #next}

[Types](03-types.md) — classes, `private`, constructors, interfaces, enums, and why there is no
inheritance.

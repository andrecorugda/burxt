---
title: Numbers and money
---

# 2. Numbers and money

## The problem, as it actually arrives

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

## Think of a bag of coins and a label

There is no `19.99` anywhere in a Burxt program's memory. There is the whole number **1999** and a
label saying *how many of these make one*. That is all a `Decimal<S>` is: an integer, and a scale
carried in its type.

<svg viewBox="0 0 640 246" role="img" aria-label="A Decimal is an integer plus a scale, and two different scales cannot be added" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .b { fill: #fff; stroke: #111; stroke-width: 1.5; }
    .d { fill: none; stroke: #b00; stroke-width: 1.5; stroke-dasharray: 5 4; }
    .t { font: 13px ui-monospace, monospace; fill: #111; }
    .n { font: 15px ui-monospace, monospace; fill: #111; }
    .g { font: 11px ui-monospace, monospace; fill: #888; }
    .s { font: 12px ui-monospace, monospace; fill: #b00; }
    .a { stroke: #111; stroke-width: 1.5; fill: none; marker-end: url(#a2); }
    @media (prefers-color-scheme: dark) {
      .b { fill: #1b1b1b; stroke: #ddd; } .t, .n { fill: #eee; }
      .s { fill: #ff8080; } .d { stroke: #ff8080; } .a { stroke: #ddd; } .g { fill: #999; }
    }
  </style>
  <defs>
    <marker id="a2" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/>
    </marker>
  </defs>

  <text class="g" x="8" y="22">you write</text>
  <text class="g" x="150" y="22">the coins</text>
  <text class="g" x="270" y="22">the label</text>
  <text class="g" x="366" y="22">so it means</text>

  <text class="t" x="8" y="56">$19.99</text>
  <rect class="b" x="150" y="30" width="80" height="40" rx="4"/>
  <text class="n" x="170" y="56">1999</text>
  <rect class="b" x="270" y="30" width="56" height="40" rx="4"/>
  <text class="n" x="292" y="56">2</text>
  <text class="t" x="366" y="56">1999 / 10² = 19.99</text>
  <path class="a" d="M74 51 L146 51"/>
  <path class="a" d="M230 51 L266 51"/>
  <path class="a" d="M326 51 L362 51"/>

  <text class="t" x="8" y="120">8.25%</text>
  <rect class="b" x="150" y="94" width="80" height="40" rx="4"/>
  <text class="n" x="178" y="120">825</text>
  <rect class="b" x="270" y="94" width="56" height="40" rx="4"/>
  <text class="n" x="292" y="120">4</text>
  <text class="t" x="366" y="120">825 / 10⁴ = 0.0825</text>
  <path class="a" d="M66 115 L146 115"/>
  <path class="a" d="M230 115 L266 115"/>
  <path class="a" d="M326 115 L362 115"/>

  <rect class="d" x="150" y="164" width="380" height="52" rx="4"/>
  <text class="t" x="166" y="188">1999 + 825 = 2824</text>
  <text class="s" x="166" y="207">2824 / 10² = 28.24 ?    2824 / 10⁴ = 0.2824 ?</text>
  <text class="g" x="8" y="238">Two labels, no shared answer. Burxt refuses. A float language picks one.</text>
</svg>

Exact for the same reason `199 + 1` is exact: there is no fraction anywhere to lose.

```burxt
let price: Decimal<2> = 19.99;      // or $19.99 — the same value
let quantity: Int     = 3;
let total: Decimal<2> = price * quantity;
print(total);                        // 59.97, exactly
```

The label lives **in the type**. `Decimal<2>` and `Decimal<4>` are different types, and the
compiler will never quietly turn one into the other.

## The literals

<div class="tablewrap" markdown="1">

| You write | It means | Its type |
|---|---|---|
| `19.99` | 19.99 | `Decimal<2>` |
| `$19.99` | 19.99 | `Decimal<2>` — the `$` is documentation, not a currency |
| `8.25%` | 0.0825 | **`Decimal<4>`** — a percent is two places finer than the number in it |
| `42` | 42 | `Int` (signed 64-bit, and it traps on overflow) |

</div>

`8.25%` being a `Decimal<4>` surprises everyone exactly once. It is arithmetic, not a rule: 8.25
percent *is* 0.0825, and 0.0825 needs four places.

A literal takes the scale of its context when that loses nothing — `let x: Decimal<2> = $5;` is
`5.00`. When it would lose something, it is refused rather than rounded.

## Adding: the labels must match

```burxt
function total(price: Decimal<2>, rate: Decimal<4>) -> Decimal<2> {
    return price + rate;
}
```

```
error: cannot + Decimal<2> and Decimal<4>: scales must match. Burxt does not
       silently rescale money.
```

Addition combines *like quantities*, and a price and a rate are not like quantities. Look at the
dashed box in the diagram again: there is no answer to give. Choosing one anyway is the silent
decision this whole language exists to prevent.

## Multiplying: the labels differ by nature, so somewhere it rounds

Multiplication is the opposite case. Multiplying an amount by a rate is the normal thing to do, and
their scales differ *by definition*:

```burxt
let price: Decimal<2> = $19.99;
let rate:  Decimal<4> = 8.25%;
let tax:   Decimal<2, RoundHalfEven> = price * rate;   // 1.65
```

2 places times 4 places is an exact product with **6**. Landing it on 2 means throwing four digits
away, so the type has to say which way: `Decimal<2, RoundHalfEven>` is a **rounding contract**.
Leave it off and you are told, with both numbers and both ways out:

```
error: this multiplication of Decimal<2> by Decimal<4> has an exact product with 6
       decimal places, and reaching Decimal<2> means rounding it. Say how —
       Decimal<2, RoundHalfEven> — or take the exact answer with Decimal<6>.
```

**Or keep every digit, which needs no contract at all**, because nothing rounds:

```burxt
let price: Decimal<2> = $19.99;
let rate:  Decimal<4> = 8.25%;
let exact = price * rate;             // Decimal<6> — 1.649175, all of it
let same: Decimal<6> = price * rate;  // the same thing, written down
```

That is the whole rule in one line: **a contract is required exactly where a value narrows.** Which
makes its presence in a program *information* — somebody made a decision right here — and that is
what it is for. ([Why it used to be demanded everywhere, and what that cost](../../spec/M10-ERGONOMICS.md).)

The two contracts are `RoundHalfEven` — banker's rounding, the default in most financial regulation
because it does not bias upward across many transactions — and `RoundHalfUp`.

**Division always needs one**, even when the scales already match, because a quotient can land
between two representable values: `1.00 / 3` has no exact answer at two places, and no scale you
could pick would give it one.

## Where to put the contract

Where the rounding happens. Not where the money enters:

```burxt
let price: Decimal<2> = $19.99;                       // no contract needed
let subtotal: Decimal<2> = price * 3;                 // exact, none needed
let rate: Decimal<4> = 8.25%;
let tax: Decimal<2, RoundHalfEven> = subtotal * rate; // ← here, where it rounds
let total: Decimal<2, RoundHalfEven> = subtotal + tax;
```

Two things make that read the way you would hope:

**A contract may be added where a value has none.** `Decimal<2>` and `Decimal<2, RoundHalfEven>`
hold the *same integer*. A contract does not reinterpret a value; it constrains what future
operations are allowed to do to it. So it can arrive at the binding that needs it.

**Adding and subtracting need matching scales, not matching contracts.** They never round, so one
side carrying a rule and the other not leaves exactly one answer to *"if this ever rounds, which
way"* — and the result carries it.

Still refused, and each for a reason you would give yourself:

<div class="tablewrap" markdown="1">

| | |
|---|---|
| `Decimal<2> + Decimal<4>` | Different labels. This is the rule that protects money |
| `Decimal<2,HalfEven> + Decimal<2,HalfUp>` | Two rules, and picking one would be a decision nobody wrote down |
| `let plain: Decimal<2> = contracted;` | **Dropping** a contract throws away a stated intention |

</div>

### A rounding rule travels

Once a value carries `RoundHalfEven`, so does every signature it flows through:

```burxt
function line_tax(subtotal: Decimal<2>, rate: Decimal<4>) -> Decimal<2, RoundHalfEven> {
    return subtotal * rate;
}
```

That looks like a cost and is the entire mechanism. A reviewer reading the *caller* can see how
this rounds without opening the file it is defined in — and an agent cannot change the rounding of
a total without changing a declaration, which is a thing [`burxt review`](01-getting-started.md)
can find.

## Integers trap, they do not wrap

`Int` is a signed 64-bit integer. Push a result past what it holds and the program stops:

```
burxt runtime error: arithmetic overflow — the exact result no longer fits in the
value range
```

In C, C#, Java and Go that same expression quietly becomes a negative number. Products are computed
in 128 bits internally here, so this error always means the *answer* does not fit — never that an
intermediate step did.

`/` on two `Int`s is a compile error, for the same reason division of money needs a contract: one
operator cannot say which way to round.

```
error: `/` on two Ints would have to round, and one operator cannot say which way —
       use divide_floor, divide_toward_zero or remainder
```

`divide_floor(-7, 2)` is `-4`. `divide_toward_zero(-7, 2)` is `-3`. They differ, they differ
*silently* in most languages, and here you say which one you meant.

## It stays exact past the edge of the program

Guarding the arithmetic and then handing the value to a C function taking a `double` guards
nothing. `as scaled` is how a `Decimal` crosses to C without becoming a float — see
[the C boundary](07-ffi.md), and [`spec/N1-BOUNDARY-EXACTNESS.md`](../../spec/N1-BOUNDARY-EXACTNESS.md)
for why the boundary is where real financial defects actually live.

## Next

[Types](03-types.md) — classes, enums, interfaces, and why there is no inheritance.

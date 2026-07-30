---
title: Contracts
description: A contract is a courier's receipt — signed on the way in, signed on the way out, and a tool can see when a signature disappears.
---

# 5. Contracts

## What this is for
{: #what-this-is-for}

A type says what **shape** a value has. `Decimal<2>` rules out a float and a string and a null. It
cannot rule out *negative*, and almost everything that goes wrong with money is a number of the
right shape and the wrong size.

So people write the rule down where they can — an `assert`, an `if` at the top of the body, a
comment — and then this happens. Somebody needs the function to accept a case it refuses. Maybe a
test, maybe an edge case at 5pm, maybe an agent that could not satisfy the check and took the
shortest path to green. They delete one line.

**Every test still passes.** In fact more of them pass, because whatever was failing was failing *on
purpose*. There is no compiler error, no warning, and nothing in the diff that looks different from
any other deleted line — because in every other language an assertion in a body *is* just another
line in a body.

That is the single most dangerous change anyone can make to a program, and this page is the reason
Burxt can see it.

## Think of a courier's receipt
{: #think-of-a-couriers-receipt}

A courier hands you a parcel and a slip of paper with two signatures on it. Yours, saying what you
handed over. Theirs, saying what came back.

Neither of you has to remember anything, and neither has to trust the other, because the slip says
both halves. And if a signature is missing later, that is not an argument about what was agreed — it is
a visibly incomplete piece of paper.

<figure>
<svg viewBox="0 0 680 250" role="img" aria-label="A contract as a two-sided receipt: requires is what the caller signs on the way in, ensures is what the function signs on the way out, and a missing signature is visible" style="max-width:100%;height:auto;">
  <style>
    .slip  { fill: #ffffff; stroke: #1d1d1f; stroke-width: 2; }
    .rule  { fill: none; stroke: #d2d2d7; stroke-width: 1; }
    .tick  { fill: none; stroke: #0f6f3c; stroke-width: 2.2; stroke-linecap: round; }
    .gone  { fill: none; stroke: #c8102e; stroke-width: 2; stroke-dasharray: 4 3; }
    .hair  { fill: none; stroke: #d2d2d7; stroke-width: 1; }
    .h     { font: 600 13.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .t     { font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #1d1d1f; }
    .lbl   { font: 600 11.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .grn   { font: 600 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #0f6f3c; }
    .red   { font: 600 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #c8102e; }
    .cap   { font: 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
  </style>

  <text class="h" x="8" y="18">The slip, with both signatures</text>

  <rect class="slip" x="14" y="32" width="290" height="150" rx="8"/>
  <line class="rule" x1="14" y1="62" x2="304" y2="62"/>
  <text class="lbl" x="26" y="53">withdraw</text>

  <text class="grn" x="26" y="84">signed on the way IN</text>
  <path class="tick" d="M26 96 l6 7 l11 -14"/>
  <text class="t" x="50" y="100">amount &gt; $0.00</text>
  <path class="tick" d="M26 118 l6 7 l11 -14"/>
  <text class="t" x="50" y="122">amount &lt;= balance</text>

  <line class="rule" x1="14" y1="136" x2="304" y2="136"/>
  <text class="grn" x="26" y="154">signed on the way OUT</text>
  <path class="tick" d="M26 166 l6 7 l11 -14"/>
  <text class="t" x="50" y="170">result &gt;= $0.00</text>

  <line class="hair" x1="336" y1="8" x2="336" y2="230"/>

  <text class="h" x="368" y="18">A signature that went missing</text>

  <rect class="slip" x="374" y="32" width="290" height="150" rx="8"/>
  <line class="rule" x1="374" y1="62" x2="664" y2="62"/>
  <text class="lbl" x="386" y="53">withdraw</text>

  <text class="cap" x="386" y="84">signed on the way IN</text>
  <path class="tick" d="M386 96 l6 7 l11 -14"/>
  <text class="t" x="410" y="100">amount &gt; $0.00</text>
  <rect class="gone" x="386" y="110" width="200" height="20" rx="4"/>
  <text class="red" x="394" y="124">amount &lt;= balance — gone</text>

  <line class="rule" x1="374" y1="136" x2="664" y2="136"/>
  <text class="cap" x="386" y="154">signed on the way OUT</text>
  <path class="tick" d="M386 166 l6 7 l11 -14"/>
  <text class="t" x="410" y="170">result &gt;= $0.00</text>

  <text class="red" x="8" y="212">Every test still passes. More of them than before.</text>
  <text class="cap" x="8" y="234">But the slip is visibly short a line — and burxt review reads slips.</text>
</svg>
<figcaption>Put the rule in the <strong>signature</strong> and it stops being a line in a body. It becomes a
two-sided promise: what the caller must guarantee before it may call, and what the function guarantees in
return. A deleted clause passes every test — more of them than before, because whatever was failing was
failing on purpose — and is still visible, because <code>burxt review</code> reads declarations.</figcaption>
</figure>

## A step closer
{: #a-step-closer}

```burxt
function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>
    requires amount > $0.00
    requires amount <= balance
    ensures result >= $0.00
{
    return balance - amount;
}
```

Three claims no type can carry, in the one place everybody already reads. `requires` is checked on the
way **in**; `ensures` on the way **out**.

<svg viewBox="0 0 640 216" role="img" aria-label="requires is checked on the way in, ensures on the way out" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .b { fill: #fff; stroke: #111; stroke-width: 1.5; }
    .gate { fill: none; stroke: #b00; stroke-width: 2.5; }
    .t { font: 12px ui-monospace, monospace; fill: #111; }
    .g { font: 11px ui-monospace, monospace; fill: #888; }
    .s { font: 11px ui-monospace, monospace; fill: #b00; }
    .a { stroke: #111; stroke-width: 1.5; fill: none; marker-end: url(#a5); }
    @media (prefers-color-scheme: dark) {
      .b { fill: #1b1b1b; stroke: #ddd; } .t { fill: #eee; } .s { fill: #ff8080; }
      .gate { stroke: #ff8080; } .a { stroke: #ddd; } .g { fill: #999; }
    }
  </style>
  <defs>
    <marker id="a5" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/>
    </marker>
  </defs>

  <rect class="b" x="8" y="70" width="128" height="44" rx="4"/>
  <text class="t" x="20" y="90">the caller</text>
  <text class="g" x="20" y="106">balance, amount</text>

  <line class="gate" x1="196" y1="46" x2="196" y2="140"/>
  <text class="s" x="150" y="36">requires</text>
  <text class="g" x="150" y="164">the caller's</text>
  <text class="g" x="150" y="178">side of it</text>

  <rect class="b" x="228" y="62" width="182" height="60" rx="4"/>
  <text class="t" x="240" y="86">withdraw</text>
  <text class="g" x="240" y="104">balance - amount</text>

  <line class="gate" x1="466" y1="46" x2="466" y2="140"/>
  <text class="s" x="428" y="36">ensures</text>
  <text class="g" x="424" y="164">the function's</text>
  <text class="g" x="424" y="178">side of it</text>

  <rect class="b" x="504" y="70" width="128" height="44" rx="4"/>
  <text class="t" x="516" y="90">result</text>
  <text class="g" x="516" y="106">&gt;= $0.00</text>

  <path class="a" d="M136 92 L190 92"/>
  <path class="a" d="M202 92 L224 92"/>
  <path class="a" d="M410 92 L460 92"/>
  <path class="a" d="M472 92 L500 92"/>

  <text class="g" x="8" y="204">neither side has to trust the other's memory — both halves are in the signature</text>
</svg>

Nothing to remember, nothing to look up in another file, and nothing that depends on anyone having
read the body.

## In code
{: #in-code}

### Named when they fail

```
burxt runtime error: `requires amount <= balance` failed in `withdraw`
```

The message **quotes the clause exactly as you wrote it**. A failure that says *precondition
violated* sends you looking for which one; quoting it means the message is already the answer. Exit
70, like every other named runtime failure.

### And now a tool can see the deletion

This is the payoff, and it only works because the promise is in the signature:

```sh
$ burxt review before.bx after.bx
WEAKENED  withdraw                           lost `requires amount <= balance`
WEAKENED  withdraw                           lost `ensures result >= $0.00`

2 weakened promise(s). A weakened contract is the one change that passes every test — the tests were failing BECAUSE of it.
$ echo $?
1
```

It **exits non-zero**, so in CI a promise cannot get quietly smaller. A *tightened* contract reports
`STRICTER` and passes; a renamed parameter reports nothing at all, because clauses are compared
structurally rather than as text.

Read that output next to the story at the top of this page. Same deletion, same green test suite —
and one line of CI that says what happened.

### There is no mode that removes them

No `--release` strips contracts. A flag that changed whether a program enforces its own stated
invariants would mean its behaviour depends on how it was built, which is the class of thing this
language refuses everywhere else. There is also no factory, wrapper or literal that gets around one
— see the [sealed box](03-types.md#piece-three-a-constructor-is-a-function-with-no-self).

Checking costs real time in a hot loop. The answer is to put contracts on **boundaries** rather than
on everything — not to make the checking optional.

### `result`, and `old(...)`

`result` is bound inside `ensures` and nowhere else. It is not a keyword: a parameter may still be
called `result`, it simply collides there, and the collision is an error rather than silent
shadowing.

`old(e)` is the value on **entry**, evaluated once before the body runs. That is what makes a
conservation law expressible — the thing an accountant would actually want checked:

```burxt
class Ledger { from_side: Decimal<2>, to_side: Decimal<2> }

function (mutable self: Ledger) transfer(amount: Decimal<2>) -> Int
    requires amount > $0.00
    ensures self.from_side + self.to_side == old(self.from_side + self.to_side)
{
    self.from_side = self.from_side - amount;
    self.to_side = self.to_side + amount;
    return 0;
}
```

Money moved; none created, none destroyed. Lose a cent anywhere in that body and it stops, quoting
the law:

```
burxt runtime error: `ensures self.from_side + self.to_side == old(self.from_side + self.to_side)` failed in `Ledger.transfer`
```

### A shorter spelling: put the claim on the value

`requires amount > $0.00` names `amount` in order to say which value it is about. Once a function has
four parameters and three claims, the reader is matching names across six lines to work out what
constrains what. So a claim can sit **on the value it is about**:

```burxt
function withdraw(balance: Decimal<2> [> $0.00], amount: Decimal<2> [<= balance]) -> Decimal<2>
{
    return balance - amount;
}
```

That is *exactly* the same program as:

```burxt
function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>
    requires balance > $0.00
    requires amount <= balance
{
    return balance - amount;
}
```

Same checks, same order, and **the same failure message down to the byte** — a test in the suite
compiles both spellings and compares the two runs, because a claim like that is worth checking rather
than asserting.

The subject is written into the message even though you did not write it:

```
burxt runtime error: `requires balance > $0.00` failed in `withdraw`
```

`balance > $0.00`, not `> $0.00`. A message that does not name the value which broke sends you back
to the declaration to find out, and that is a cost paid on every failure forever.

A bracket on the **return type** is an `ensures`, and its subject is `result`:

```burxt
function fee(amount: Decimal<2> [> $0.00]) -> Decimal<2> [>= $0.00] {
    return amount;
}
```

#### A bracket is a list of claims

The comma is **and**. `||` is **or**. Parentheses group.

```burxt
function banded(v: Decimal<2> [it > $0.00, (it < $1000.00 || it > -$100.00)]) -> Decimal<2> {
    return v;
}
```

That is **two** claims, not three: the second is one claim with an `||` inside it. Break it and the
message quotes it whole, parentheses included:

```
burxt runtime error: `requires (v < $1000.00 || v > -$100.00)` failed in `banded`
```

Which is the reason the comma exists at all, since you could always write one `&&` instead:
**`[a, b]` tells you which one broke; `[a && b]` tells you only that something did.** Same check,
worse message — the same argument as the synthesized subject.

Two more things follow from the comma being *and*:

**Clauses are checked left to right, and the first failure wins.** `[it > 0, it > 10, it > 100]`
given `5` reports `n > 10` — not `n > 0`, which passed, and not `n > 100`, which was never reached.

**A comma inside a call is not a separator.** This is two clauses, not four:

```burxt
pure function between(v: Int, lo: Int, hi: Int) -> Bool {
    return v > lo && v < hi;
}

function ranged(n: Int [between(it, 0, 100), it != 42]) -> Int {
    return n;
}
```

#### `it`, when the value appears twice

The leading form only reaches the left of a comparison. When the value is needed anywhere else, name
it `it`:

```burxt
function band(balance: Decimal<2> [it > $0.00 || it > -$100.00]) -> Decimal<2> {
    return balance;
}

function shout(word: String [len(it) > 0]) -> String {
    return word;
}
```

`it` is resolved in the message too — `band` reports `balance > $0.00 || balance > -$100.00`, for the
same reason as above.

And `it` is **not a keyword**. Outside a bracket the name is free:

```burxt
function count() -> Int {
    let it: Int = 7;
    return it;
}
```

A function with a parameter *called* `it` and a bracket that *says* `it` has two meanings for one
word, so that is refused rather than silently shadowed — the same rule `result` follows inside
`ensures`.

Spreading the elision across `||` was considered and rejected: `[a > 0 || > 1]` would be a rule you
had to remember, and remembering rules is what this language tries not to charge you for.

### `pure` — an answer that depends on its arguments and nothing else

```burxt
pure function fee_for(amount: Decimal<2>) -> Decimal<2, RoundHalfEven> {
    return amount * 2.50%;
}
```

A `pure` function may not print, read or write a file, call into C, or call a function that is not
itself `pure`. It is a claim, and the compiler holds you to it:

```burxt
pure function fee_for(amount: Decimal<2>) -> Decimal<2, RoundHalfEven> {
    print("computing");
    return amount * 2.50%;
}
```

```
error: `pure function fee_for` may not print: a pure function's result must depend only on
       its arguments, which is the whole of what `pure` promises. Pass the value in as a
       parameter instead.
```

**Contract clauses are checked under that same rule**, and that is the real reason `pure` exists
here. A clause that could print, mutate or call out would be a second program that only runs when
somebody is looking — and a check that can change the answer is not a check.

`pure` and [`touches`](06-effects.md) are the same claim from opposite ends, so saying both is a
contradiction rather than a refinement, and the compiler says so.

### `decreases` — this recursion ends

```burxt
function countdown(n: Int, acc: Int) -> Int
    decreases n
{
    if n <= 0 { return acc; }
    return tail countdown(n - 1, acc + n);
}
```

The measure is evaluated **with the new arguments at each recursive call** and compared against the
current one: strictly smaller, and never negative.

```
burxt runtime error: `decreases n` did not decrease on a recursive call to `walk`
```

Checking at the *call site* rather than on entry is what makes this work with `return tail`, where
the frame that would have remembered the old value is already gone.

The measure must be an `Int`. A `Decimal` measure invites a descent that shrinks forever without
arriving — `1.00`, `0.50`, `0.25`, `0.125` — which is the exact failure the clause exists to rule
out.

## Why it is built this way
{: #why-it-is-built-this-way}

**A claim in a signature is a claim a tool can read.** That is the entire reason contracts are not
`assert` in a body. `assert amount <= balance` is a line among lines: delete it and the diff shows one
plausible removal. `requires amount <= balance` is part of the declaration, so deleting it changes what
the function *promises*, and [`burxt review`](12-tools-and-agents.md) exits non-zero.

**The message is the answer.** A failure quotes the clause exactly as you wrote it, rather than saying
*precondition violated* and leaving you to find which one.

**And the same clause does a second job.** `burxt mcp-schema` derives an MCP tool's JSON Schema from
these preconditions — so the bound an agent is validated against and the bound the function enforces are
one sentence. That is only possible because the claim is in the signature.

**There is no mode that removes them.** No `--release` strips contracts. A flag that changed whether a
program enforces its own stated invariants would mean its behaviour depends on how it was built, which
is the class of thing this language refuses everywhere else.

## What it costs
{: #what-it-costs}

**Checking costs real time in a hot loop.** The answer is to put contracts on **boundaries** rather than
on everything — not to make the checking optional.

**They are runtime checks, not proofs.**

Static proof of arbitrary contracts is SMT-solver
territory, and a prover that is right *sometimes* is worse than a check that is right *always*.
Static proof is the eventual goal; this is what is reachable and true today.
([The design record, including what a static pass would need.](https://github.com/andrecorugda/burxt/blob/main/spec/A5-CONTRACTS.md))

**An `ensures` cannot bind `result` to a class yet.** A class travels back through a hidden pointer
into the caller's storage, and binding `result` to that needs care a scalar does not. Return a scalar, or
drop the clause.

**A clause relating two parameters has no JSON Schema key.** It is still enforced; `burxt mcp-schema`
reports that it could not carry it rather than approximating. See
[tools and agents](12-tools-and-agents.md).

## When you reach for it
{: #when-you-reach-for-it}

<div class="tablewrap" markdown="1">

| You want to say | Write |
|---|---|
| this argument must be positive | `Int [> 0]`, or `requires n > 0` |
| this amount must not exceed that one | `requires amount <= balance` — a bracket cannot relate two parameters |
| the answer is never negative | `-> Decimal<2> [>= $0.00]`, or `ensures result >= $0.00` |
| this call changed the balance by exactly the amount | `ensures self.balance == old(self.balance) - amount` |
| this function reads nothing and touches nothing | `pure function` |
| this recursion ends | `decreases n` |
| a bound an agent calling this tool must respect | put it on the value: `mcp-schema` reads it |

</div>

Put them on **boundaries** — the edge of a module, a constructor, anything an agent will call. A
contract on every private helper costs time and tells a reviewer nothing new.

## Examples
{: #examples}

**A contract firing, and naming itself.** The bracket form and the `requires` form are the same
sentence, so this uses the short one:

```burxt
function withdraw(balance: Decimal<2>, amount: Decimal<2> [> $0.00, <= balance])
    -> Decimal<2> [>= $0.00]
{
    return balance - amount;
}

print(withdraw($100.00, $30.00));
print(withdraw($100.00, $200.00));
```

```
70.00
burxt runtime error: `requires amount <= balance` failed in `withdraw`
```

The first call answers. The second stops the program, and the message **quotes the clause you wrote**
rather than saying which of three it might have been.

**`pure` and `decreases`, together.** `pure` says the answer depends on the arguments and nothing else;
`decreases n` says this recursion ends:

```burxt
pure function factorial(n: Int [>= 0, <= 20]) -> Int
    decreases n
{
    if n <= 1 { return 1; }
    return n * factorial(n - 1);
}

print(factorial(5));
print(factorial(10));
```

```
120
3628800
```

The `<= 20` is not decoration: `21!` does not fit in an `Int`, and an overflow would stop the program.
The clause turns that into a refusal at the call instead.

## Next
{: #next}

[Effects](06-effects.md) — what a function is allowed to reach, and why that belongs in the
signature too.

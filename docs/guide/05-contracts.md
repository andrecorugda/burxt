---
title: Contracts
---

# 5. Contracts

## The problem, as it actually arrives

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

## A contract is a handshake, written down

Put the rule in the **signature** and it stops being a line in a body. It becomes a two-sided
promise: things the *caller* must guarantee before it may call, and things the *function* guarantees
in return.

```burxt
function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>
    requires amount > $0.00
    requires amount <= balance
    ensures result >= $0.00
{
    return balance - amount;
}
```

Three claims no type can carry, in the one place everybody already reads.

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

`requires` is checked on the way **in**; `ensures` on the way **out**. Nothing to remember, nothing
to look up in another file, and nothing that depends on anyone having read the body.

## Named when they fail

```
burxt runtime error: `requires amount <= balance` failed in `withdraw`
```

The message **quotes the clause exactly as you wrote it**. A failure that says *precondition
violated* sends you looking for which one; quoting it means the message is already the answer. Exit
70, like every other named runtime failure.

## And now a tool can see the deletion

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

## There is no mode that removes them

No `--release` strips contracts. A flag that changed whether a program enforces its own stated
invariants would mean its behaviour depends on how it was built, which is the class of thing this
language refuses everywhere else. There is also no factory, wrapper or literal that gets around one
— see the [sealed box](03-types.md#piece-three-a-constructor-is-a-function-with-no-self).

Checking costs real time in a hot loop. The answer is to put contracts on **boundaries** rather than
on everything — not to make the checking optional.

## `result`, and `old(...)`

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

## A shorter spelling: put the claim on the value

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

### A bracket is a list of claims

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

### `it`, when the value appears twice

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

## `pure` — an answer that depends on its arguments and nothing else

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

## `decreases` — this recursion ends

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

## What is not claimed

These are **runtime** checks, not proofs. Static proof of arbitrary contracts is SMT-solver
territory, and a prover that is right *sometimes* is worse than a check that is right *always*.
Static proof is the eventual goal; this is what is reachable and true today.
([The design record, including what a static pass would need.](../../spec/A5-CONTRACTS.md))

## Next

[Effects](06-effects.md) — what a function is allowed to reach, and why that belongs in the
signature too.

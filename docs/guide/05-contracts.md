---
title: Contracts
---

# 5. Contracts

A type says what shape a value has. A contract says what must be **true** about it.

```burxt
function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>
    requires amount > $0.00
    requires amount <= balance
    ensures result >= $0.00
{
    return balance - amount;
}
```

Three claims no type can carry, written where a reader looks for them: in the signature.

## Checked at run time, and named when they fail

```
burxt runtime error: `requires amount <= balance` failed in `withdraw`
```

The message **quotes the clause as written**. A failure that says "precondition violated"
makes you go find which one; quoting it means the message is the answer. Exit 70, like every
other named runtime failure.

## There is no mode that removes them

No `--release` strips contracts. A flag that changes whether a program enforces its own
stated invariants would mean the program's behaviour depends on how it was built, which is
the class of thing this language refuses everywhere else.

This costs real time in a hot loop, and the answer is to put contracts on **boundaries**
rather than on everything — not to make the checking optional.

## `result`, and `old(...)`

`result` is bound inside `ensures` and nowhere else. It is not a keyword: a parameter may
still be called `result`; it simply collides there, and the collision is an error rather
than silent shadowing.

`old(e)` is the value on **entry**, evaluated once before the body runs. It is what makes a
conservation law expressible:

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

Money moved, none created, none destroyed — and a transfer that loses a cent fails by name.

## `pure` — an answer that depends on its arguments alone

```burxt
pure function fee_for(amount: Decimal<2>) -> Decimal<2> { ... }
```

A `pure` function may not print, read or write a file, call into C, call a function that is
not `pure`, or call a method at all. It is a claim the compiler checks.

**Contract clauses are checked under the same rule**, and that is the point: a contract that
can change the program is not a check, it is a second program that only runs when someone is
looking.

## `decreases` — this recursion ends

```burxt
function countdown(n: Int, acc: Int) -> Int
    decreases n
{
    if n <= 0 { return acc; }
    return tail countdown(n - 1, acc + n);
}
```

The measure is evaluated **with the new arguments at each recursive call** and compared with
the current one: strictly smaller, and never negative. Checking at the *call site* rather
than in the callee is what makes it work with `return tail`, where the frame that would have
remembered the old value is already gone.

The measure must be an `Int`. A `Decimal` measure invites a descent that shrinks forever
without arriving — `1.00`, `0.50`, `0.25` — which is the exact failure the clause exists to
rule out.

## What is not claimed

These are **runtime** checks, not proofs. Static proof of arbitrary contracts is SMT-solver
territory, and a prover that is right sometimes is worse than a check that is right always.
Static proof is the eventual goal; the runtime form is what is reachable and true today.

## Next

[Effects](06-effects.md) — what a function can reach, and why that belongs in the signature too.

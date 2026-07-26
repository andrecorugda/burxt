# Burxt — Termination As A Contract (NOVELTY §5, slice 1)

> Status: **specified, to implement.** `NOVELTY.md` §5 asks for a `decreases`
> measure, noting it is *"an extension of §3 rather than a separate idea"* and
> pairing with it: **one says the answer is right, the other says an answer
> arrives.** §3's runtime form shipped in v0.0.43–v0.0.44; this is the same staging
> for §5.

## 0. The claim

> **This recursion ends. The compiler checks, on every call.**

An infinite loop in a payment processor is a real failure mode, not an academic
one — and today the honest answer in every language is that nothing checks. A
`decreases` clause names a quantity that must shrink on every recursive call:

```text
fn sum_to(n: Int, acc: Int) -> Int
    decreases n
{
    if n <= 0 { return acc; }
    return tail sum_to(n - 1, acc + n);
}
```

Dafny and ACL2 prove such measures statically. This checks them at runtime, for the
same reason §3's contracts are checked at runtime: it is reachable now, it is never
wrong when it fires, and it does not pretend to a proof it cannot produce.

## 1. Decisions

### Decision 1 — the check happens at the CALL SITE, not in the callee

At a recursive call, the measure is evaluated **with the new arguments** and compared
against the measure of the invocation making the call. Both are known right there.

The alternative — each invocation recording its measure somewhere for the next one to
read — needs per-function state that must be restored on the way out, and **a
guaranteed tail call has no way out to restore from**: the frame is gone. Checking at
the call site works with `return tail` for free, needs no global state, and is
correct under recursion of any depth.

### Decision 2 — the measure must be an `Int`

A count, not a quantity. A `Decimal` measure invites a descent that shrinks forever
without arriving — `1.00`, `0.50`, `0.25` — which is exactly the failure the clause
exists to rule out. Requiring an integer makes "strictly smaller, and never
negative" a real ladder to the floor.

### Decision 3 — two conditions, both checked

```text
burxt runtime error: `decreases n` did not decrease on a recursive call to `sum_to`
burxt runtime error: `decreases n` is negative in `sum_to`
```

- **Strictly smaller** at every recursive call. Equal is a failure: equal measures
  are how a loop that never ends looks.
- **Never negative**, checked on entry. A measure that can fall below zero is not
  a ladder to the floor; it is a hole.

### Decision 4 — the measure may only mention parameters

It is checked in the same scope contracts are, which contains the parameters and
nothing else. A measure that reads mutable state outside the call would not be a
function of the arguments, so the substitution at the call site would be a lie.

It is also checked under the `pure` rule (v0.0.39), for the reason contract clauses
are: a measure that can change the program is not a check.

### Decision 5 — direct recursion only, and say so

`f` calling `f` is checked. **`f` → `g` → `f` is not**, because the two functions
would need a shared measure and there is nothing to compare `g`'s state against.
Stated in the error-free case too — the clause does not claim more than it checks —
and deferred with a trigger rather than silently half-working.

## 2. What this must NOT do

- **NO static proving in this slice.** Same reasoning as §3: SMT territory, and a
  prover that is right sometimes is worse than a check that is right always.
- **NO stripping the check in a build mode.** Same as contracts: a program's
  enforcement of its own claims must not depend on how it was built.
- **NO `decreases` on a non-recursive function.** It would be a claim with nothing
  to check, and a reader would reasonably assume it meant something.
- **NO measure that is not an `Int`.** See Decision 2.
- **NO mutual-recursion claim.** See Decision 5.

## 3. Deferred

| Feature | Why deferred | Earns its place when |
|---|---|---|
| Mutual recursion (`f` → `g` → `f`) | Needs a measure shared across a group | A required program recurses mutually |
| `decreases` on methods | Same plumbing as contracts on methods, one step behind | A required program needs one |
| Lexicographic measures (`decreases a, b`) | One integer covers the cases that exist | A required program needs a pair |
| Static proof | SMT territory | The runtime form has proven the grammar |
| `decreases` on a `while` loop | Loops are not the failure mode recursion is; a loop's bound is visible | A required program has an unbounded loop |

## 4. Acceptance

1. A recursive function with `decreases` compiles and runs when the measure shrinks.
2. It works with `return tail`, which is the case the design was chosen for.
3. A recursive call that does not shrink the measure dies with exit 70, naming the
   clause and the function.
4. A measure that goes negative dies with exit 70, naming the clause.
5. A non-`Int` measure is a compile error.
6. An impure measure is a compile error.
7. `decreases` on a function that never calls itself is a compile error.
8. `decreases` composes with `requires`, `ensures` and `pure` on one signature.

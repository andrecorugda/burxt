# Burxt — contracts on the values they constrain (M13)

> Status: **stage-0 DONE (v0.0.135). Stage-1 pending, with one identified obstacle.**
>
> Purely additive: `requires` and `ensures` keep working and keep their meaning, and the whole suite
> passes unchanged — 35 invariants, fixpoint intact, not one existing contract touched.
>
> The desugaring is observable rather than asserted. The same constraint written both ways produces
> **byte-identical failure messages**:
>
> ```
> function f(b: Decimal<2>, a: Decimal<2> [<= b]) -> ...
> function f(b: Decimal<2>, a: Decimal<2>) -> ... requires a <= b
> ```
> both →  `` burxt runtime error: `requires a <= b` failed in `f` ``
>
> That is why the recorded clause text is the clause *as a reader would write it*, subject included:
> an elided `[<= b]` reports `a <= b`, not a fragment that does not say which value broke.
>
> ### The obstacle in stage-1, stated so it is not rediscovered
>
> Parameter brackets are straightforward there: a parameter has a real name token, so the synthesized
> subject can point at it.
>
> **The return bracket cannot, and the reason is structural.** Stage-1 names every binding by its
> SPAN in the source — the same constraint that shaped three earlier designs. Its checker binds
> `result` with `find_text(src, "result")`, literally searching the source for the word. A bracket
> form never writes `result`, so there is nothing to find and nothing to point a synthesized node at.
>
> The fix is a synthetic token whose span the checker agrees to treat as `result`, which is real work
> rather than a patch — and it is the same wall that killed the `for` desugar in M10. Until it is
> done, stage-0 has both forms and stage-1 has neither, exactly as M7 staged generics.

## 0. What is wrong with the current form

```burxt
function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>
    requires amount > $0.00
    requires amount <= balance
    ensures result >= $0.00
{
    return balance - amount;
}
```

Nothing is *incorrect* here. Two things are awkward, and Andre named both from a user's seat:

1. **The clauses are a list you match to parameters by eye.** `amount <= balance` is a fact about
   `amount`, but it lives three lines below `amount`, next to facts about other things. With four
   parameters and six clauses you are doing a join in your head.
2. **The `{` drifts away from the signature.** A six-line header means you cannot see where the body
   starts, and the function looks bigger than it is.

## 1. The design

```burxt
function withdraw(
    balance: Decimal<2> [> $0.00, < $10.00],
    amount:  Decimal<2> [<= balance],
) -> Decimal<2> [>= $0.00] {
    return balance - amount;
}
```

**A bracket after a type is a list of clauses about the value of that type.** On a parameter it is a
precondition; on the return type it is a postcondition. Comma means *and*.

### Decision 1 — the subject is ELIDED, never named

`[> $0.00]`, not `[self > $0.00]`.

This is Andre's improvement on the first sketch, and it fixes a collision the sketch had missed:
inside a method, `self` is the receiver, so `[self > $0.00]` hung on a parameter is genuinely
ambiguous — receiver or parameter? Eliding removes the question rather than answering it. **Position
decides the subject: it is the thing the bracket is attached to**, and there is no way to write it
otherwise.

The rule, in one sentence: **a clause that begins with a comparison operator gets the subject
inserted on its left.**

### Decision 2 — `it` names the subject when a clause needs it twice

Elision only reaches the leading position, so this does not work:

```burxt
[(> $0.00 || < $10.00)]          // the subject would have to appear in TWO places
```

Distributing the elision across `||` and `&&` was considered and rejected: it is a rule you would
have to remember, and it would make `[a > 0 || > 1]` mean something surprising. Instead, name it:

```burxt
[it > $0.00 || it < $10.00]
[len(it) > 0]
[it != old(it)]
```

`it` follows the rule `result` already follows in `ensures` — **not a keyword, but it collides.** A
parameter may still be called `it`; inside a contract bracket that is an error about the collision
rather than silent shadowing. Consistency with a decision this language already took beats a new
mechanism.

### Decision 3 — each clause stays separate, and that is the point of the comma

`[> $0.00, < $10.00]` is **two clauses**, not one `&&`. So when one fails the message names the one
that failed:

```
burxt runtime error: `> $0.00` failed for `amount` in `withdraw`
```

An `&&` would only be able to say the whole conjunction broke. This is the same argument the
existing contract machinery already makes by quoting clause text, extended to the comma.

### Decision 4 — the types stay in the signature

Andre also proposed hoisting them out, leaving `function withdraw(balance, amount)` with the types
in attributes above. Rejected, and the reasoning is worth keeping because the idea is attractive:

**An empty signature moves the answer out of the only place every reader looks for it.** A call site
you jump from, a hover, signature help, and "what does this file's API look like" all stop being
answerable by reading the declaration. It also works against *"typing is good"* — Andre's own
opening praise for the language.

There was a second problem: `#[@balance]` binds by **name across a gap**. Rename the parameter and
the attribute refers to nothing; nothing enforces that the attribute list matches the parameter
list. That is one fact in two places, the failure `spec/A7.0-NAMING.md` exists to prevent.

### Decision 5 — no `|>`

The first sketch had `balance: Decimal<2> |> [> $0.00]`. Once the brackets delimit the clause the
arrow does no work, and `|>` is the **pipeline** operator in F#, Elixir and OCaml — so it would
mislead every reader arriving from one of those.

### Decision 6 — additive, and `requires` is the escape hatch

`requires` and `ensures` are unchanged and remain the underlying form. Brackets **desugar** to them,
which is why this milestone touches the parser and almost nothing else: the checking, the failure
messages, the purity rule, `old()` and the exit-70 behaviour are all machinery that already works.

The escape hatch is not a courtesy. **A precondition relating three parameters belongs to none of
them**, and attaching it to whichever one makes it typecheck would be arbitrary:

```burxt
function transfer(from: Account, to: Account, amount: Decimal<2>)
    requires from.id != to.id
{ ... }
```

That clause is about the *call*, not about a value. `requires` says so. Brackets are for the common
case where a constraint genuinely belongs to one value, and forcing them onto the rest would be the
kind of purity that makes people write worse code to satisfy a syntax.

## 2. What the desugaring is

| Written | Means |
|---|---|
| `x: Int [> 0]` | `requires x > 0` |
| `x: Int [> 0, < 10]` | `requires x > 0` and `requires x < 10`, separately |
| `-> Int [>= 0]` | `ensures result >= 0` |
| `-> Int [self.a + self.b == old(self.a + self.b)]` | `ensures self.a + self.b == old(...)` |
| `x: Int [it != 0]` | `requires x != 0` |

The return bracket is the "on exit" slot, so a conservation law lives there even though it is not
about the returned value. One slot that sometimes means more than the result beats two mechanisms
that overlap.

## 3. Acceptance

1. Both compilers accept it, and the **byte-identical fixpoint holds**.
2. A pass fixture proving the two forms are equivalent — the same program written both ways,
   producing identical output, so the desugaring is checked rather than asserted.
3. A fail fixture per refusal: a bracket clause that is not `pure`, a clause naming `it` where a
   parameter is also called `it`, and an elided clause that does not begin with a comparison.
4. The failure message names the clause AND the value it was about — `\`> $0.00\` failed for
   \`amount\`` — which is strictly more than the current message gives.
5. `docs/guide/05-contracts.md`, `examples/contracts.bx` and the reference updated in the same
   commit, because a syntax nobody can find is a syntax nobody uses.
6. Existing contract programs are untouched. If a single `requires` in the suite has to change, this
   milestone stopped being additive and the spec is wrong.

## 4. What this must NOT do

- **NO removing `requires` or `ensures`.** §1 Decision 6. They are the underlying form and the
  escape hatch, and the three-parameter case has no bracket to live on.
- **NO hoisting types out of the signature.** §1 Decision 4.
- **NO distributing the elided subject** across `||` or `&&`. Decision 2 — write `it`.
- **NO new checking machinery.** If the desugaring needs anything the clause path does not already
  do, the desugaring is wrong.
- **NO silent shadowing of `it`.** A parameter named `it` collides inside a bracket, and says so.

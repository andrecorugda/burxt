# Burxt — contracts on the values they constrain (M13)

> Status: **stage-0 DONE except Decision 2 (v0.0.135). Stage-1 pending, one obstacle left.**
>
> **Two corrections, both found in v0.0.166 by finally writing a fixture.**
>
> 1. **Decision 2 — `it` — is NOT implemented.** `[it * 2 > 0]` answers `unknown variable: it`. The
>    parser's comment says `it` is "resolved later against a binding this function installs nowhere.
>    The checker is what knows it means the subject" — and the checker does not. Naming the parameter
>    works; `it` is specified and absent. Acceptance item 3's fail fixtures for it do not exist either.
>
> 2. **The bracket form had NO test coverage at all**, for fourteen versions. `src/parser.rs` cited a
>    `tests/pass/contract_brackets.bx` that had never been written. The claim below that the
>    desugaring is "observable rather than asserted" was true of neither: nothing observed it and
>    nothing asserted it. It is now `bracket_contracts_desugar_to_the_same_message` in
>    tests/runner.rs, which compiles both spellings and compares the failure text, plus
>    `tests/stage0-only/contract_brackets.bx` for the accepting side.
>
> Worth stating plainly, because it is the second time in two days: a status line saying DONE is not
> evidence. The suite is.
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
>
> ### A SECOND obstacle, found v0.0.147, and it is the harder one
>
> Parameter brackets are **not** straightforward after all, and the reason is the clause TEXT
> rather than the subject. The subject is fine — a parameter has a real name token, so a
> synthesized comparison can point at it.
>
> But stage-1 stores a clause's text as a **byte span into the source**
> (`parse_clauses` records `text_start` / `text_length`), because a failure has to quote what the
> programmer wrote and a span costs nothing. Stage-0 builds that text as a **String**, which is
> what lets it synthesize `a <= b` from an elided `[<= b]`.
>
> In `balance: Decimal<2> [> $0.00]` the subject and the clause are **not contiguous** — the type
> sits between them. So no span can spell `balance > $0.00`, and §1 Decision 3's whole point is
> that the message must name the value that broke:
>
> ```
> burxt runtime error: `> $0.00` failed in `withdraw`     ← what a span can say
> burxt runtime error: `balance > $0.00` failed in ...    ← what acceptance item 4 requires
> ```
>
> Three ways out, none of them small, and the choice is a design decision rather than a detail:
>
> 1. **Give stage-1 synthesized clause text.** It would need somewhere to keep built Strings for
>    clause text instead of spans — the honest fix, and it touches how every clause is stored.
> 2. **Quote the written form in BOTH compilers** — `balance: Decimal<2> [> $0.00]`, verbatim,
>    subject included because the whole declaration is contiguous. Arguably better than a
>    synthesized rewrite, and it costs acceptance item 4 as currently written plus the fixture
>    that proves the two spellings produce byte-identical messages.
> 3. **Let the messages diverge.** Rejected on sight: two compilers, one language, and the
>    `.stderr` fixtures pin one text.
>
> Nothing was built for M13 in stage-1. This note exists so the next attempt starts from the real
> problem rather than the one the spec used to describe.
>
> ### v0.0.165: the wall is narrower than this note claimed
>
> Two of the three things blocked on "stage-1 cannot hold a synthesized name" turned out not to be.
>
> **Constructors were not blocked at all.** `Account.open` looked unreachable because it is not
> contiguous in the source — and stage-1 already keys a METHOD by TWO spans, a receiver and a name,
> because that is how `Account` + `shown` is found. A qualified name *is* a name in two parts. So an
> associated function is a method with a flag and no receiver argument, the existing lookup finds it,
> and no name is built in either compiler. Shipped in v0.0.165 as roughly forty lines.
>
> The lesson generalises and is worth carrying into the next attempt at this: **the constraint is not
> "every name is a span", it is "every name is a span, and a name may be more than one of them."**
> Before reaching for a synth buffer, ask whether the name in question is already written down in
> pieces somewhere.
>
> **The clause text (obstacle 2) is not helped by that.** Option 2 — quote the written form in both
> compilers — was offered and **DECLINED by Andre in v0.0.165**:
>
> > *"`balance > $0.00` this one cause it is understandable that this comes from a contract"*
>
> Which settles it, and the reason is the one this whole language is organised around: the message a
> reviewer reads has to say **which value broke**. `[> $0.00]` on its own does not, and asking a
> reader to look back at the declaration to find out is the kind of small tax that gets paid on every
> failure forever. The synthesized form stands, and stage-1 has to produce it.
>
> ### What that costs, decomposed (v0.0.165)
>
> Stage-0 builds `format!("{} {}", subject, written)` where `written` is the bracket's own span. So
> the text is **two pieces**, which is the shape v0.0.165 already showed stage-1 can hold — a thing
> that is not one span may still be two.
>
> | Piece | Parameter bracket | Return bracket |
> |---|---|---|
> | the subject, in the message | the parameter's name span | the constant `"result"` — a compiler string, in no program |
> | the clause, in the message | its own span | its own span |
> | the subject, in the CONDITION | a `Var` node at the name token | **nothing to point at** |
>
> Three of those four are free. The last one is the whole remaining wall, and it is narrower than a
> synth buffer: the elided `[>= $0.00]` on a return type needs an operand meaning *the value being
> returned*, and nothing has to NAME that. A dedicated node kind — the checker answers the enclosing
> function's return type, the emitter loads the result slot — is smaller than the synthetic token this
> note originally proposed, and does not require stage-1 to invent a span.
>
> Note also that `emit_ensures` currently finds the result slot with `find_text(src, "result")`,
> literally searching the program for the word. That works only because the longhand form writes it.
> The slot needs an identity that is not a span before the bracket form can emit.

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

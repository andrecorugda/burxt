# Burxt — Contracts, Runtime-Checked (A5 slice 1, NOVELTY §3 staging)

> Status: **slice 1 SHIPPED in v0.0.43; `old(...)` and method contracts SHIPPED in
> v0.0.44**, which makes NOVELTY §3's conservation laws expressible and checked. §1's
> "cannot express a conservation law" no longer holds and is kept below for the
> record, marked.
>
> Original status: **specified, to implement.** `NOVELTY.md` §3 wants conservation laws
> proven statically and says so honestly: *"static proof of arbitrary contracts is
> SMT-solver territory... Runtime-checked contracts plus derived locking is
> reachable much sooner and is worth shipping first."* This is that first step, and
> it is also A5's `requires` / `ensures` from the roadmap.

## 0. What a contract is for

A type says what shape a value has. A contract says what must be **true** about it.

```text
function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>
    requires amount > $0.00
    requires amount <= balance
    ensures result >= $0.00
{
    return balance - amount;
}
```

Three claims that no type in the language can carry, written where the reader looks
for them — in the signature, not in a comment and not in the body.

## 1. Decisions

### Decision 1 — checked at runtime, and named when they fail

```text
burxt runtime error: `requires amount <= balance` failed in `withdraw`
```

The message quotes **the clause as written**. A contract failure that says
"precondition violated" makes the reader go find which one; quoting it means the
message is the answer. Exit 70, like every other named runtime failure — bounds,
overflow, division by zero, region exhaustion.

### Decision 2 — always checked, with no mode that removes them

There is no `--release` that strips contracts. A flag that changes whether a program
enforces its own stated invariants would mean the program's behaviour depends on how
it was built, which is the class of thing this language refuses everywhere else. If
a contract is too expensive to check, it should not be a contract.

This is a real cost and it is chosen deliberately: a `requires` in a hot loop is
work on every call. The answer is to write contracts on boundaries rather than on
everything, not to make the checking optional.

### Decision 3 — `ensures` sees `result`

The name `result` is bound to the returned value inside `ensures` clauses, and
nowhere else. It is not a keyword: a parameter or binding may still be called
`result` — it simply collides inside the clause, which is an error naming the
collision, since shadowing is refused everywhere in Burxt.

### Decision 4 — contracts must be pure

A clause is checked under the same rule `pure function` enforces (v0.0.39): no printing,
no file reads, no FFI, no calls to functions that are not `pure`. **A contract that
can change the program is not a check, it is a second program**, and one that only
runs when someone is looking.

This falls out of machinery that already exists, which is the second time the
`allocates`/`pure` markers have paid for themselves.

### Decision 5 — `old(...)` is deferred, and that means conservation laws are too

> **Superseded in v0.0.44.** Both pieces shipped: contracts on methods, and
> `old(...)` hoisted out of the clause and evaluated once on entry. The paragraph
> below is what was believed when the first slice shipped, kept because the reasoning
> was right — it just turned out to be one version of work rather than several.

§3's headline example needs the *pre-state*:

```text
ensures from.balance + to.balance == old(from.balance + to.balance)
```

That needs values captured at entry and compared at exit, and it only means anything
for functions that MUTATE — which today means methods with a `mutable self` receiver,
since Burxt's function parameters are by-value copies. Both pieces are real work and
neither is needed to make `requires`/`ensures` useful. Deferred with a trigger,
stated plainly rather than half-built: **this slice cannot express a conservation
law.** It can express bounds, ranges, sign, and relations between arguments and
result, which is most of what contracts are used for in practice.

## 2. What this must NOT do

- **NO stripping contracts in any build mode.** See Decision 2.
- **NO impure clauses.** See Decision 4.
- **NO static proving in this slice.** Deciding `amount <= balance` at compile time
  is SMT territory; pretending otherwise would mean a checker that is right
  sometimes, which is worse than a check that is right always.
- **NO `ensures` on a function returning an aggregate**, yet: the result travels via
  a hidden pointer into the caller's storage, and binding `result` to it needs care
  that scalars do not. Refused with the reason, not silently ignored.
- **NO contracts on `external function`.** Burxt cannot check the other side, and a
  precondition on a C function would be a claim it never agreed to.
- **NO inheritance-style contract weakening/strengthening rules.** Those belong with
  `class`/`open`, which do not exist.

## 3. Deferred

| Feature | Why deferred | Earns its place when |
|---|---|---|
| ~~`old(expr)` in `ensures`~~ | **DONE** (v0.0.44) | — |
| ~~Contracts on methods~~ | **DONE** (v0.0.44) | — |
| ~~Conservation laws (§3's headline)~~ | **DONE** (v0.0.44) | — |
| `old(...)` of an aggregate | Needs a copy of the whole value at entry | A required program needs the whole record, not fields |
| Derived mutual exclusion from an invariant (§3's novel step) | Needs threads | Concurrency exists |
| Static proof | SMT territory | The runtime form has proven the grammar |
| `ensures` on aggregate returns | `result` binding needs sret care | A required program needs it |

## 4. Acceptance

1. A function with `requires` and `ensures` compiles and runs when they hold.
2. A violated `requires` dies with exit 70 and a message quoting the clause and
   naming the function.
3. A violated `ensures` does the same.
4. Several clauses of each kind are allowed, and the FIRST one to fail is the one
   reported.
5. A non-Bool clause is a compile error.
6. An impure clause is a compile error naming purity.
7. `result` in a `requires` clause is a compile error: there is no result yet.
8. `ensures` on a function returning a record is refused with the reason from §2.
9. Contracts compose with `pure` and `allocates` on the same signature.

## 4a. Two rules relaxed (v0.0.86)

Andre's judgement, after the rule cost three attempts on a seven-line invoice: it was too
strict, and in a way that put the fix somewhere other than the error.

**A rounding contract may be ADDED where a value has none.** `Decimal<2>` and
`Decimal<2, RoundHalfEven>` hold the same integer; a contract does not reinterpret a value,
it constrains what future operations may do to it. So `let tax: Decimal<2, RoundHalfEven> =
subtotal * rate;` works with a plain `Decimal<2>` subtotal, and the contract can live where
the rounding happens rather than where money entered the program.

**Addition and subtraction need matching SCALES, not matching contracts.** They never round,
so one side carrying a contract and the other carrying none leaves exactly one answer to "if
this ever rounds, which way" — and the result carries it.

What did NOT change, because these are the rules that protect money:

- **Scales must still match** on `+` and `-`.
- **Two different contracts are still refused**, with a message that says picking one would
  be a decision nobody wrote down.
- **Dropping** a contract is still refused: that loses a declared intention.
- `*` with mixed scales and `/` always still demand a contract.

Both compilers, and `tests/pass/contract_widening.bx` / `tests/fail/mixed_rounding.bx` pin
the new boundary from both sides. `examples/invoice.bx` lost three contract annotations it
only ever carried to satisfy the old rule.

## 5a. Acceptance for `old(...)` and method contracts (v0.0.44)

10. A `mutable self` method carries `requires` and `ensures`, checked like a function's.
11. `ensures self.a + self.b == old(self.a + self.b)` holds for a transfer that
    conserves, and fails — quoting the clause and naming the method — for one that
    loses a cent.
12. `old(...)` is evaluated ONCE on entry, before the body runs and before the
    preconditions are checked, so a failing precondition reports the state as it
    arrived.
13. More than one `old` in a clause, and more than one clause using `old`, both work.
14. `old(...)` outside an `ensures` clause is refused with the reason.
15. `old(result)` is refused as a contradiction.
16. `old(...)` of an aggregate is refused, naming the copy that is not built.
17. `old` is a reserved name: `function old(...)` is refused.

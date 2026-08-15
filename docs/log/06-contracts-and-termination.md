# Contracts, conservation laws, and termination

*Milestone log, v0.0.43 – v0.0.50. The design these versions serve is in [DESIGN.md](../../DESIGN.md); the whole log is indexed [here](README.md).*

`requires` and `ensures` checked at runtime, `old(...)` making conservation laws expressible, `decreases` checked at every recursive call, integer division by name, inheritance dropped — and a use-after-free the escape checker had accepted since regions shipped.

### v0.0.43: contracts — `requires` and `ensures`, checked

```text
fn withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>
    requires amount > $0.00
    requires amount <= balance
    ensures result >= $0.00
{
    return balance - amount;
}
```

A type says what shape a value has. A contract says what must be **true** about it —
three claims no type in the language can carry, written where a reader looks for what
a function demands and promises rather than in a comment or buried in the body.

**When one fails, the message quotes it:**

```text
burxt runtime error: `requires amount <= balance` failed in `withdraw`
```

Not "precondition violated" — that makes the reader go and find which one, and there
is usually more than one. Exit 70, like every other named failure: bounds, overflow,
division by zero, region exhaustion.

**Always checked, with no mode that removes them.** There is no `--release` that
strips contracts. A flag deciding whether a program enforces its own stated
invariants would make behaviour depend on how it was built, which is the class of
thing this language refuses everywhere. The cost is real and chosen: a `requires` in
a hot loop is work on every call, and the answer is to put contracts on boundaries
rather than on everything.

**`ensures` sees `result`**, bound to the value about to be returned, and **every
return is checked** — not only the last one. `result` is not a keyword: a binding may
still be called that, it simply collides inside the clause, which is an error naming
the collision because Burxt does not shadow. In a `requires` clause `result` is
refused with the reason: *"it is checked on entry, before there is a result."*

**Contracts must be pure, and that fell out of machinery that already existed.** A
clause is checked under exactly the rule `pure fn` enforces (v0.0.39): no printing,
no file reads, no FFI, no impure calls. **A clause that can change the program is not
a check, it is a second program that runs only when someone is looking.** That is the
second time the effect markers have paid for themselves — `pure` was built on
`allocates`, and contracts are built on `pure`.

**A wording bug worth recording.** Reusing the purity checker meant a bad clause was
reported as *"`pure fn f` may not call `log` ... or drop `pure` from `f`"* — on a
function that never declared `pure`. Nonsense advice, produced by borrowing a
mechanism and inheriting its vocabulary. There is now a flag distinguishing
*checking a clause* from *checking a pure body*, and the clause version says what it
means.

**What this slice deliberately cannot do: express a conservation law.** NOVELTY §3's
headline needs `old(...)` — values captured at entry and compared at exit — and that
only means anything for functions that MUTATE, which today means methods with a `mut
self` receiver. Both are real work; neither is needed for `requires`/`ensures` to be
useful for bounds, ranges, sign and relations between arguments and result. Stated
plainly rather than half-built, with a trigger in the spec.

Also refused, with reasons rather than silence: `ensures` on a function returning an
aggregate (the result travels by hidden pointer, so binding `result` needs care a
scalar does not), and static proving, which is SMT territory — a checker that is
right sometimes is worse than a check that is right always.

Spec: `spec/1.0/A5-CONTRACTS.md`.

### v0.0.44: conservation laws, checked (NOVELTY §3's headline)

```text
fn (mut self: Ledger) move_to_savings(amount: Decimal<2>) -> Int
    requires amount > $0.00
    requires amount <= self.checking
    ensures self.checking + self.savings == old(self.checking + self.savings)
```

**That last line is the invariant that actually defines correctness for a ledger** —
money moves, and nothing is created or destroyed. It is not a comment and not a test;
it is part of the signature, and every call checks it. When a version of the same
method loses a cent on the way:

```text
burxt runtime error: `ensures self.checking + self.savings == old(self.checking +
self.savings)` failed in `Ledger.leaky_move`
```

The message quotes **the law itself**, which is the point: the reader sees the
invariant that broke, not a line number.

Two pieces landed to get here, and v0.0.43 predicted both would take longer.

**Contracts on methods.** The same clauses, on the receiver-and-parameter scope. A
*mutating* method is where contracts earn the most, because it is the only place in
the language where the state can differ before and after.

**`old(...)`, hoisted rather than re-evaluated.** The expressions inside `old` are
lifted out of the clause by the typechecker, evaluated **once on entry**, and stored;
the clause reads what was stored. Order matters and is deliberate: captures happen
before the preconditions are checked, so a failing `requires` reports the state as it
arrived, and before any of the body runs, or the values would not be "old" at all.

`old` is refused where it would be meaningless, each with its reason: outside an
`ensures` clause (there is no entry to refer back to), `old(result)` (the state before
the call had no result), and `old` of an aggregate (copying a whole struct at entry is
not built — take `old` of a field, or of a sum of fields). It is also a reserved name
now, so `fn old(...)` cannot shadow it.

**A process failure worth recording, because it cost real time.** I checked build
results with `cargo build | grep -c '^(error|warning)'` and read the answer — `2` — as
two warnings. They were two *errors*. So for several minutes I tested a **stale
binary**, watched a conservation law silently not fire, and went looking for the bug
in the parser, the typechecker and the code generator in turn. All three were fine.

The lesson is exact: **never gate on a count that cannot distinguish success from
failure.** `grep -c` was chosen to keep output short, and it removed the one
distinction that mattered. The suite has a rule about this for the language — errors
must name themselves — and I broke it in my own tooling.

### v0.0.45: `decreases` — termination the compiler checks (NOVELTY §5)

```text
fn sum_to(n: Int, acc: Int) -> Int
    decreases n
{
    if n <= 0 { return acc; }
    return tail sum_to(n - 1, acc + n);
}
```

The register pairs §5 with §3 exactly: **one says the answer is right, the other says
an answer arrives.** A `decreases` measure names a quantity that must shrink on every
recursive call — and an infinite loop in a payment processor is a real failure mode
that nothing else checks for.

**The design decision that made this small: check at the CALL SITE.** At a recursive
call the measure is evaluated *with the new arguments* and compared against the
calling invocation's measure. Both are known right there.

The obvious alternative — each invocation recording its measure for the next one to
read — needs per-invocation state that must be restored on the way out, and **a
guaranteed tail call has no way out to restore from**: the frame is gone. Checking at
the call site works with `return tail` for free, needs no global state, and is correct
at any depth. Two of my own features would otherwise have collided.

**And the substitution costs nothing.** The measure is written in terms of the
parameters, so evaluating it for the callee means binding the parameter names to the
argument values and generating the same expression again. No rewriting, no
substitution pass over the AST — just a shadowed scope around one `gen_expr`.

**Two conditions, because one is not enough.** Strictly smaller at every call (equal
is how a loop that never ends looks), and never negative — a measure that can fall
below zero is not a ladder to the floor, it is a hole.

**The measure must be an `Int`**, and the error says why: a `Decimal` measure can
shrink forever without arriving — `1.00`, `0.50`, `0.25` — which is precisely the
failure the clause exists to rule out.

**A bug avoided, and worth recording because it nearly shipped.** The measure check
needs the *Burxt* argument values, while the call already had ABI-shaped ones
(truncated `CInt`s, converted doubles, an `sret` slot occupying index 0). My first
version simply generated the arguments a second time for the measure — which would
have run their **side effects twice**. Now each argument is generated once and kept in
both shapes.

Refused with reasons rather than silence: a non-recursive function with a measure (a
claim with nothing to check reads as if it meant something), two measures (that would
be a lexicographic measure, which is not built), an impure measure, and `decreases` on
a method — one step behind contracts on methods, which shipped last version.

**Honest limit, stated in the clause's own spec:** direct recursion only. `f` → `g` →
`f` is not checked, because the two would need a shared measure and there is nothing
to compare `g`'s state against.

Spec: `spec/1.0/N5-TERMINATION.md`.

### v0.0.46: integer division by name, and inheritance dropped

Two decisions taken, one adding a feature and one removing a plan.

**`div_floor`, `div_trunc`, `rem` — and `/` on two Ints stays refused.**

```text
print(div_floor(-7, 2));   // -4, rounds down
print(div_trunc(-7, 2));   // -3, rounds toward zero
```

Integer division had been refused outright since v0.0.2, which was right about the
danger and wrong about the remedy: compiler-shaped code needs midpoints, counts and
byte arithmetic, and forcing a rounding contract onto an array index is absurd. But
**one operator cannot say which way it rounds**, and the answers differ on negatives
— which is exactly the kind of difference that must not hide behind a symbol. So the
operation is named, the way `byte_at` is named for bytes:

```text
error: `/` on two Ints would have to round, and one operator cannot say which way:
       -7 divided by 2 is -3 rounding toward zero and -4 rounding down. Say which you
       mean — `div_floor(a, b)`, `div_trunc(a, b)`, or `rem(a, b)`.
```

Each form checks what C leaves **undefined**: division by zero, and `i64::MIN / -1`,
whose quotient does not exist in an i64. Both are named runtime errors with exit 70,
like every other one. `rem` pairs with `div_trunc` (its sign follows the dividend); a
flooring remainder is deferred until something needs it.

**`class` and `open` single inheritance are dropped. Composition-only is final.**

The reason is evidence rather than taste. Traits + `impl` + composition shipped in
v0.0.13–v0.0.14, and across everything since — regions, sum types, contracts,
conservation laws, termination measures, a self-hosted lexer and parser — **nothing
has needed inheritance. Not once.** An item that sits on a roadmap for thirty
versions without a single program asking for it is not planned, it is a wish, and the
rule here is that a feature earns its place by being needed.

What the plan was reaching for, the language already has: reuse from composition,
substitutability from traits, and no fragile base class or diamond problem because
there is no base class. The superseded design is kept in DESIGN.md as the record of
what was considered — including the SOLID table, where Liskov moves from
"contract-checked" to something stronger: **unrepresentable to violate**, since a
type satisfies a trait exactly or it is a compile error, and there is no subtype to
weaken a contract.

### v0.0.47: `substring`, allocating methods, and a symbol table in Burxt

The self-hosting track, and it behaved exactly the way this track is supposed to:
**writing real Burxt found real gaps.**

**`substring(s, at, len)`** — a copy of part of a String, in the current region,
NUL-terminated, so the result is an ordinary Burxt String: comparable, joinable,
printable, and passable to C. Bounds are checked against the source and the failure
names the numbers:

```text
burxt runtime error: substring(s, 2, 5) does not fit — this string has 3 bytes
```

Why this was the blocker rather than a convenience: a lexer could already *compare* a
span against a keyword byte by byte, which is why keyword matching worked without it.
What it could not do was **keep** a name — and a symbol table is made of kept names.

**A symbol table, written in Burxt** (`examples/symbols.bx`). It reads a real `.bx`
file, finds every `let NAME: TYPE`, interns the names, and reports a redeclaration —
the same rule the Rust typechecker enforces:

```text
declared `price` : Decimal
declared `qty` : Int
redeclared: `qty` was already declared at offset 171
--- 4 names in scope
```

This is the first piece of the *typechecker* to be self-hosted, after the lexer
(v0.0.21) and the parser (v0.0.22).

**Two findings it produced, which is the point of the exercise.**

**1. Burxt has no mutable parameters, and that is now a stated decision rather than
an accident.** `fn collect(src: String, mut table: Table, ...)` does not parse:
mutation goes through a `mut self` receiver. So a pass that fills a table has to *be a
method on the table*. Discovered by writing the obvious thing and having it refused.

Kept as-is deliberately. One way to mutate — through a receiver, callable only on a
`let mut` binding — is the rule the whole aggregate ABI was built around (v0.0.14's
correction: receivers pass as a plain pointer, ordinary aggregates as `byval` copies).
Adding `mut` parameters would mean two mechanisms with different aliasing stories, and
it would quietly undo the property that a function cannot alter its caller's values.
The constraint also pushes code toward methods, which matches the OOP-by-default
stance rather than fighting it.

**2. `allocates` on methods, which the M1a spec had deferred with the trigger "a
required program needs an allocating method".** The symbol table was it: `collect`
builds names with `substring` and messages with `to_string`, so it must allocate in the
caller's region. Implemented for methods exactly as for functions — the flag is hoisted
with the signature, call sites are checked for an open region, and a call to one counts
as allocating at the call site, so the caller's escape rules govern the result.

A trigger firing on its own, from a program written for another reason, is the
deferred-features ledger working as designed.

### v0.0.48: the escape checker was blind to aggregates

**A soundness hole, and how it was found matters as much as the fix.**

Writing the next self-hosted piece meant deciding how a Burxt checker would report an
error, and the natural answer is an enum: `Outcome { Good(Ty), Bad(String) }`. Which
raised a question about my own compiler — *can that message get out of the region it
was built in?* It could:

```text
struct Named { word: String }

fn take(src: String) -> Named {
    region inner {
        return Named { word: substring(src, 0, 3) };   // accepted. Dangling.
    }
}
```

`no errors`. The region closes at the brace, the struct leaves holding a pointer into
released storage, and reading it is a use-after-free — **exactly the silent wrongness
this language exists to refuse**, sitting in the checker meant to prevent it.

The cause was narrow and dull: `expr_allocates` walks an expression asking "did this
build region storage?", and it knew about concatenation, `substring`, `to_string`,
`read_file`, `push`, and calls to `allocates` functions — but not about **aggregates
that contain any of those**. A struct literal, an enum variant and an array literal
were all transparent to it.

Three arms, and the hole is closed in every form: struct field, enum payload, array
element. Both directions are now tested — the refusals, and the case that must keep
working, which is that **inside** a region an aggregate may hold region storage freely.
That is what a symbol table *is*; only carrying it out is refused.

**Why this is a good argument for self-hosting as a method rather than a milestone.**
The hole had existed since regions shipped (v0.0.24) and survived 280 test programs,
because every test that returned an aggregate returned one built from scalars and
literals. It took writing a *program with a real design question* to walk into it. The
lexer rewrite found three wrong assumptions, the parser rewrite corrected a
milestone-blocking claim, and this one found a memory-safety bug. That is three for
three.

### v0.0.49: the scale rule, enforced by Burxt

`examples/checker.bx` reads a real `.bx` file and refuses what the language refuses:

```text
let broken : Decimal<2>
  cannot apply `+` to Decimal<2> and Decimal<4>: addition combines like quantities,
  so the scales must match
let tax : Decimal<2>
  `*` on Decimal<2> and Decimal<4> needs a rounding contract on the result: the
  exact product has 6 decimal places
let tax_ok : Decimal<2, RoundHalfEven>          <- accepted
let mixed : Int
  type mismatch: declared Int, but the expression has type Decimal<2>
```

**This is the thesis checking itself.** Not the arithmetic — a Burxt program applying
Burxt's own scale rules to Burxt source: addition needs matching scales, a Decimal
product needs a rounding contract, and the product's exact scale is the sum of the
operands' (2 + 4 = 6, computed by the checker).

Types are a sum type here, as they are in the Rust compiler: `enum Ty { Unknown, IntTy,
BoolTy, StringTy, Dec(Int, Bool) }` — scale, and whether a contract was written. Struct
fields hold those enums, a growable array holds those structs, and every name and type
in the table is a `substring` of the source.

**One bug in the Burxt code, worth keeping because it is a real typechecker lesson.**
The first version suppressed cascades with `!ty_eq(found, Ty.Unknown)` and printed a
second complaint for every first one. `ty_eq` answers *false* for `Unknown` against
anything — deliberately, because **an unknown type must never compare equal to
anything, including another unknown**, or one bad expression makes every later
comparison succeed. Suppressing the cascade therefore needs its own predicate,
`is_unknown`. The Rust compiler learned the same distinction two versions ago from the
other end, when recovery needed a failed `let` to still bind its declared type.

**Where self-hosting now stands:** lexer (v0.0.21), parser (v0.0.22), symbol table
(v0.0.47) and the scale rule (v0.0.49) are written in Burxt — 600-odd lines of it,
compiled by the Rust compiler and run against real source files.

**The next constraint is already visible, and it is not a missing feature:** Burxt has
**no module system**, so `checker.bx` carries its own copy of `is_alpha`, `skip_spaces`
and `word_at` rather than sharing the lexer's. One file works, and the real self-hosted
compiler will be one file until imports exist. Recorded rather than fixed: a module
system is a design question about namespaces and compilation units, and it earns its
place when a single file stops being tolerable rather than when it stops being pretty.

### v0.0.50: `break` and `continue`, earned by evidence

These had been on the deferred list since v0.0.11 with the note "nothing has needed
them yet, so they stay deferred rather than speculative". Then the self-hosted code
started working around their absence, in two different ways:

```text
let mut running: Bool = true;      // examples/lexer.bx — a flag to leave a loop
while running { ... running = false; ... }

let mut guard: Int = 0;            // examples/symbols.bx — a counter to bound one
while cursor < len(src) && guard < 10000 { ... guard = guard + 1; }
```

That is the ledger's rule working: **a feature earns its place when a program needs
it**, and three programs needed this one. All three now say what they mean, and the
workarounds are gone.

**The interesting part was regions.** A jump out of a loop has the same problem
`return` had in v0.0.29 — if a `region` was opened inside the loop, leaving it must
release the region, or the bump cursor climbs forever. But a region that *encloses*
the loop must **not** be released, because the jump stays inside it. Guessing would
be wrong half the time, so the loop records what was open when it started, and the
jump compares: region open now, none open at loop entry ⇒ it was opened inside ⇒
release it.

The test for that runs 30,000 iterations, each opening a region and leaving it by
`continue`. Without the release it dies of region exhaustion; with it the memory is
reused. That is the same shape as the v0.0.29 test, for the same reason.

**One distinction that mattered more than it looks.** `break` ends a block, so code
after it is unreachable — but it must **not** satisfy a function's obligation to
return a value. Conflating the two would accept `fn f() -> Int { while true { break; } }`.
So there are now two questions asked of a statement: *does control leave it*
(`stmt_diverges`, used for unreachable code) and *does it return a value*
(`stmt_returns`, used for the return-path proof). A test asserts the second still
refuses a function that ends in `break`.

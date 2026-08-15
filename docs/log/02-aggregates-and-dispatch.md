---
layout: doc
title: Aggregates, dispatch, and the literals money needs
section: log
description: *Milestone log, v0.0.11 – v0.0.20. The design these versions serve is in DESIGN.md; the whole log is indexed here.*
---

# Aggregates, dispatch, and the literals money needs

*Milestone log, v0.0.11 – v0.0.20. The design these versions serve is in [DESIGN.md](../../DESIGN.md); the whole log is indexed [here](README.md).*

The aggregate ABI, receiver methods, interfaces and `dyn`, the logical operators, string length and equality, interpolation, money and percent literals, mixed-scale multiplication, and sum types with exhaustive matching.

### v0.0.11: honest numbers, unary minus, human errors

A 97-program adversarial sweep found six open issues; all fixed. The numeric
three shared one root cause worth stating plainly: **the scaled-i64
representation's cost is not precision, it is that INTERMEDIATES need more
headroom than values do.**

- **i128 intermediates.** The double-scale product (`A*B`), the pre-scaled
  dividend (`A*10^S`), and the tie test (`2*|r|`) now compute in i128, where
  they cannot overflow; only the final narrowing back to i64 is checked. So
  `40000000.00 * 40000000.00` and `Decimal<18> / Decimal<18>` work, and the
  overflow error now means what it says — before, it fired on results that
  fit perfectly, which is worse than the abort because it misdirects
  debugging.
- **The most negative decimal prints.** The print path splits in i128 and
  shows magnitudes with `%llu`, so a value you can compute and store is no
  longer unprintable.
- **`Decimal<0>` prints no phantom `.0`.** Scale 0 has no fractional digits;
  showing one contradicted "prints exactly".
- **Unary minus exists.** `print(-19.99)` works, negation is
  overflow-checked, and negated literals stay literals. Previously the only
  way to negate was `0 - x`, and the test suite carried throwaway zero
  bindings to do it — the language telling on itself.
- **Deep nesting no longer aborts the compiler — INTERIM MEASURE.**
  Compilation runs on a 512 MB-stack thread. Measured ceiling: ~30,000
  operator terms compile, ~40,000 abort (SIGABRT, exit 134 — loud, but it is
  still an abort); nested parens survive 50,000+. Before this, 1,500 terms
  died.

  This buys headroom; it does not remove the limit, so it must not be
  mistaken for the answer. **TODO: make the AST WALKERS iterative** —
  explicit work-stack in `typeck::check_expr` and `codegen::gen_expr`, plus a
  manual `Drop` for `Expr`/`TypedExpr`. Note where the recursion actually
  is: the parser is already iterative for operator chains
  (`parse_additive`/`parse_term` are loops), and parens create no AST nodes
  at all — which is why 50,000 parens are fine. What overflows is walking a
  50,000-deep left-nested `Binary` tree, so a "make the parser iterative"
  fix would be aimed at the wrong stage. Depth should ultimately be bounded
  by heap, not stack, because generated code (and the self-hosted compiler's
  own output) will not respect a 30,000-node budget.
- **Errors read as English.** Token names come from a `describe()` table
  (`expected \`;\`, found the end of the file`), never Rust `Debug` dumps,
  and chained comparisons get their own message instead of mentioning
  `RParen`.

### v0.0.12: the aggregate ABI (A4.5)

How multi-field values cross function boundaries. Unglamorous plumbing, but
it is the substrate the object model sits on, so it is settled BEFORE the OOP
grammar rather than rewritten underneath it.

**The principle that decides everything else:** semantics are defined at the
value level; the ABI is a mechanism that must be invisible to them. Passing,
returning and assigning an aggregate are value-copy operations on every
target. Whether the machine uses registers or a pointer to a temporary is a
target detail the program can never observe — if a program could tell the
difference, the ABI is wrong, not the program.

- **Parameters.** Scalars in registers, as before. Aggregates as LLVM
  `byval(T)`: a pointer to a caller-owned copy, so the callee's pointer never
  aliases the caller's live storage. LLVM guarantees the copy — hand-rolling
  by-value-through-pointer is exactly where aliasing bugs live, so we don't.
  Inside the callee the incoming pointer IS the binding's slot; writing
  through it is safe by construction.
- **Returns.** Aggregates always use an `sret(T)` hidden first pointer — one
  code path on every target, and the only shape wasm can express. A
  register-pair fast path is deliberately NOT implemented: it is a per-target
  size-classification problem, LLVM often recovers the cost anyway, and it
  can be added later as a pure optimization behind unchanged semantics. The
  scalar/aggregate boundary is decided by the TYPE, never by size, so it is
  target-independent.
- **Layout is exactly the declared fields** — declaration order, standard
  alignment padding, and NOTHING else: no type tag, no vtable pointer, no
  refcount, no hidden header. A field's offset is a pure function of the
  declared types and order, so adding a trait implementation later cannot
  move a field. `burxt layout <file>` prints size/align/offsets, and a test
  asserts them, so this guarantee is machine-checked rather than promised.
- **Arrays pass as a pointer plus a static length** — N lives in the type, so
  it costs no runtime argument and `len()` stays a compile-time constant.
  Semantically still a value copy.
- **The correctness test that keeps this honest:** a callee scribbles over its
  aggregate parameter and the caller's value is verified unchanged, for a
  one-field struct AND an eight-field struct — i.e. across both plausible
  mechanisms. Identical behavior either way is the property; if switching
  mechanism ever changes observable behavior, that is the bug.

What A4.6 may assume, and no more: layout is the declared fields; dispatch
data lives OUTSIDE the value (fat pointer / dictionary); pass/return/assign
is a value copy on every target; `field N` denotes the same field everywhere.
Needing to violate one of those is a signal to revisit this milestone
deliberately and record it — not to bolt a hidden header onto the layout.

### v0.0.13: receiver methods — the first slice of A4.6

```text
struct Account { balance: Decimal<2> }

fn (self: Account) balance_of() -> Decimal<2> {
    return self.balance;
}
fn (mut self: Account) deposit(amount: Decimal<2>) -> Decimal<2> {
    self.balance = self.balance + amount;
    return self.balance;
}

let mut acct: Account = Account { balance: 100.00 };
acct.deposit(25.50);           // side effect kept, result discarded
print(acct.balance_of());      // 125.50
```

A method is a plain function in the receiver's namespace — `fn (self: T)
name(...)`, mangled `bx.<T>.<name>` — not an impl block, so there is no hidden
`this` and no new nesting form. `self` is bound exactly like a parameter, with
the SAME exact-type rules everywhere else.

- **Two receiver forms, and the aggregate ABI already built them both.**
  `self: T` passes as `byval(T)` — a value copy, like any struct parameter;
  mutating it inside the method can never be observed by the caller. `mut
  self: T` is the one place Burxt passes an aggregate by address ON PURPOSE:
  no `byval`, so the pointer is the caller's real storage. Nothing new had to
  be invented for either — non-mutating methods reuse the v0.0.12 `byval`
  path unchanged, and mutating methods reuse the existing field-assignment
  address logic.
- A mutating method may only be called on a `let mut` binding, and only on a
  plain variable — not an expression, which has no caller storage to mutate.
  This is the exact rule `item.field = value` already enforces, applied to
  `self`.
- Methods are namespaced by `(receiver, name)`, so two structs may each
  declare a method with the same name; resolution is by the base value's
  type, decided at compile time (there is no dispatch yet — that is A4.6's
  next slice, interfaces).

**Also landed: expression statements.** Methods exposed a real gap — there
was no way to call anything purely for its side effect; every call had to be
wrapped in `print(...)` or `let`. `f();` and `acct.deposit(10.00);` are now
statements: `Stmt::ExprStmt`, evaluated for effect, result discarded.

### v0.0.14: interfaces and dispatch (A4.6)

```text
trait HasBalance {
    fn balance_of(self) -> Decimal<2>
}

impl HasBalance for Account {
    fn (self: Account) balance_of() -> Decimal<2> { return self.balance; }
}

print(acct.balance_of());          // static: a direct call, no vtable exists
let any: dyn HasBalance = acct;    // `dyn` is the ONLY thing that asks for
print(any.balance_of());           // runtime dispatch
```

A trait is a named set of method signatures a type can promise to satisfy.
That is the whole concept: no fields, no state, no bodies.

- **Satisfaction is explicit and nominal.** `impl Trait for Type { ... }`.
  Burxt never auto-satisfies a trait because method shapes happen to match, so
  conformance is a deliberate, greppable declaration — and adding a trait
  method later cannot silently un-satisfy a type that never opted in. The
  check is exact: every method present, same receiver form, same types. A
  partial impl names the missing method.
- **Static by default, dynamic only when asked.** A trait-method call on a
  known concrete type is a direct call and emits no vtable — verified by a
  test that greps the IR. Write `dyn Trait` and you get a fat pointer
  `(data, vtable)` with runtime dispatch. If you never write `dyn`, you never
  pay for one. Performance is legible in the syntax.
- **This is where the A4.5 layout guarantee gets spent.** The vtable is static
  read-only data OUTSIDE the value, one table per (Type, Trait) actually used
  as `dyn`, holding function pointers in trait-declaration slot order. So a
  struct's field offsets are byte-identical whether or not it is ever a trait
  object — a test asserts exactly that.
- **One ABI correction the milestone forced.** Methods now take `self` as a
  plain pointer, never `byval`. A vtable slot cannot name a concrete type, so
  it cannot carry `byval(T)`; with byval receivers a direct call (struct
  lowered into registers) and an indirect call (pointer) disagreed about the
  ABI, which produced silently wrong values. This is sound because a
  non-mutating `self` is read-only — the typechecker refuses `self.field =`
  without `mut self` — so a pointer to the caller's storage is
  indistinguishable from a pointer to a copy. Ordinary aggregate parameters
  keep `byval`; only the receiver changed.
- A trait object **borrows** its data, and Burxt has no borrow tracking, so
  the sound subset is enforced: it must be built from a variable, may be a
  parameter (Dependency Inversion — depend on the contract, not the struct),
  but may not be returned, stored in a struct field, or re-borrowed from
  another trait object. Each refusal says why.
- A trait object exposes **only** its trait methods. There is no downcasting
  and no way to ask what concrete type it really is.

### A4.6 deferred-features ledger

Each of these is a real feature other languages have, and each is deferred
with a trigger rather than silently added — because in a design phase with no
physics to enforce discipline, scope is the thing being tested.

| Feature | Why deferred | Earns its place when… |
|---|---|---|
| Default methods (bodies in traits) | Pulls in override-resolution: "which body runs" | A required program needs shared default behavior across many impls |
| Trait inheritance (`trait A : B`) | Biggest complexity multiplier — satisfaction becomes a graph problem | A real hierarchy genuinely cannot be modeled by small separate traits |
| Generics / trait bounds | A whole milestone: inference, monomorphization | Static polymorphism over type parameters is concretely needed |
| Associated types / constants | Compounds generics complexity | Only alongside generics, if ever |
| Blanket / overlapping impls | Coherence and overlap rules | A required pattern cannot be expressed one impl at a time |
| Operator traits (`Add`) | Backdoors operator overloading into the numeric core | Never, unless the numeric stance is deliberately revisited |
| Downcasting / reflection | Large surface, breaks the abstraction | A required program genuinely needs runtime type identity |
| Multiple dispatch | Dispatch is on the single receiver only | — |
| Mutating methods through `dyn` | Needs to know the borrowed value is itself mutable — that is a borrow checker | Borrow tracking exists |
| Returning / storing a `dyn` | Borrows would outlive their storage | Borrow tracking exists |

`interface` remains a reserved word (v0.0.8) but `trait` is the chosen
keyword; the North Star's `struct X is Priceable` sketch is superseded by
`impl Trait for Type`, which gives the trait's methods one definite home and
a place to enumerate them for a vtable.

### v0.0.15: `&&`, `||`, `!` — closing A5.0

The last gap in the control-flow milestone. Bool-only, no truthiness: the
left and right of `&&`/`||` must each be a `Bool`, and `!` refuses anything
else, each with an error that says why.

```text
if balance > 0.00 && balance < 1000.00 { ... }
if n != 0 && d / n > 1.00 { ... }        // safe: the division never runs
```

**Short-circuit is part of the language, not an optimization**, because
skipping the right side is observable. It lowers to real basic blocks with a
phi at the join, never a bitwise `and` of both sides. Two tests pin this down:
one where the right side is a function that prints (its number is absent when
skipped), and one where the right side would divide by zero — it prints
instead of trapping, which is only possible if the right side genuinely does
not execute.

`&` and `|` alone are errors pointing at `&&`/`||`; Burxt has no bitwise
operators, so the single forms are free to be advice instead of silently
meaning something else.

With this, A5.0's own acceptance program passes: `fib(20)` prints `6765`.
Still deliberately deferred from that spec: `for` (needs iterators),
`match` (needs sum types), `break`/`continue` (nothing has needed them yet),
and any form of ternary.

### v0.0.16: string length and equality (A4.4, unblocked half)

```text
print(len("hello"));        // 5
print("abc" == "abc");      // true
```

The audit in `spec/README.md` found that A4.4 bundled length, equality and
concatenation as one heap-blocked group, but only concatenation actually needs
the heap. Length is a byte scan; equality is a byte loop. Both shipped here;
**concatenation stays refused** until the memory model (M1).

- **`==` on String is the SAME `==`**, not a parallel string-equals path. It
  slots into the existing one-equality-no-coercion rule exactly as `Bool` does:
  equality yes, ordering still refused, and a cross-type comparison falls
  through to the shared catch-all, so `"a" == 1` reads identically to
  `1 == 1.00`. Getting this right while String has few operations is much
  cheaper than retrofitting it later.
- **Equality is by BYTES, never pointer identity.** Two identical literals
  become two separate globals (`@str`, `@str.1`) with different addresses, and
  a test asserts they still compare equal.
- **`len` now spans arrays and strings, and the two are different kinds of
  length** — worth keeping visible: an array's length lives in its TYPE and
  folds to a compile-time constant, while a string's is a property of its DATA
  and is scanned at runtime. A test rebinds a `let mut` String to prove the
  string form is genuinely not constant-folded.
- Both helpers (`@burxt.strlen`, `@burxt.streq`) are generated loops rather
  than calls to libc `strlen`/`strcmp`. A builtin must not quietly consume a C
  symbol name — `extern fn strlen` stays available to user code, and an
  existing test still uses it — and nothing here depends on libc for a future
  wasm target.

### v0.0.17: string interpolation, and the syntax-change law

```text
print("Account of {owner}: {balance}");    // Account of Andre: 1234.56
print("literal braces: \{like this\}");
```

`{expr}` splices a value's exact display form; any expression works, obeying
exactly the grammar and type rules it would outside a string.

**The law this milestone established**, because a bare `{` was previously an
ordinary character:

> A feature that changes what currently-valid syntax MEANS must make the old
> form a compile error, never silently reinterpret it. The breaking error,
> pointing at the exact fix, is the feature working correctly.

So `print("hi {name}")` — which used to print the braces literally — is now a
compile error naming `\{` as the fix, rather than quietly becoming
interpolation. Letting it change meaning silently was rejected on principle:
a language that reinterprets valid syntax once will do it again, and that is
the whole trust proposition. All brace handling lives in the lexer's string
scanner, so `{`, `}`, `\{`, `\}` are settled at tokenization and the parser
has no ambiguity left to resolve.

**Interpolation prints; it does not build a String.** Producing a String value
would mean building new bytes — the same allocation wall concatenation hits —
so `let s: String = "x {n}";` is refused with that reason, while
`print("x {n}")` emits the pieces in order and allocates nothing. This keeps
the milestone on the safe side of the heap boundary; it lifts with M1.

Literal pieces remain printf ARGUMENTS, never format strings, so a `%s` or
`%n` in user text still prints literally.

### v0.0.18: money and percent literals (A4.7, slice 2)

```text
let price: Decimal<2> = $19.99;      // sugar for Decimal<2>, not a new type
let rate:  Decimal<4> = 8.25%;       // exactly 0.0825
```

- `$19.99` is a `Decimal<2>` literal. `$5` and `$5.5` widen exactly to `5.00`
  and `5.50`; `$1.999` is refused, because `$` means scale 2 and dropping the
  third digit would lose money. Every existing decimal rule applies unchanged,
  since this is sugar over the existing type rather than a new one.
- `8.25%` is **exactly** 0.0825 — the same digits with two more decimal
  places, never a division by 100 and never a float. A percentage's type is
  therefore two scales wider than the percentage as written.
- **No type inference.** `let price = $19.99;` is still an error. Per the
  binding amendment in the A4.7 spec, how much inference a language has shapes
  every line of every program, so it gets its own milestone rather than
  arriving as a side effect of a dollar sign.

**Open decision this surfaced** (recorded in the A4.7 spec): the spec's own
flagship `price + price * 8.25%` at `Decimal<2>` does NOT compile, because a
percent literal is `Decimal<4>` and `Decimal * Decimal` requires matching
scales. Percent-of-money works at a matching scale today; making the mixed
form work means deciding whether `*` may take mixed scales when a rounding
contract says how to land — a change to a core thesis rule, so it is left as
a judgment call rather than smuggled in.

### v0.0.19: mixed-scale multiplication — percent-of-money works

```text
let price: Decimal<2, RoundHalfEven> = $19.99;
let total: Decimal<2, RoundHalfEven> = price + price * 8.25%;   // 21.64
```

**A refined rule, not a silent swap.** `*` now permits operands of DIFFERENT
scales when the result binding supplies a rounding contract. `+` and `-`
remain strict. The asymmetry is principled and states in one sentence:

> Addition combines like quantities, so scales must match. Multiplication
> combines a quantity with a rate, so scales differ by nature.

`$1.00 + $0.001` is almost always a bug — differing scales suggest the operands
are not the same kind of thing. But money × rate is inherently between two
different kinds: a price is scale-2, a tax or interest rate is finer. Forcing
those to match was forcing a fiction.

**The mandatory contract is the safety line and must never become optional.**
A mixed-scale product's natural scale is the SUM of the operand scales, so
landing it on the declared scale drops digits, and someone must say how. Without
a contract on the result it is a compile error naming the fix. If a mixed-scale
product could be silently rounded, the thesis would be broken.

This is *more* on-thesis than staying strict, not a loosening: the strict form
already "worked" by making the author widen money to the rate's scale by hand,
multiply, and narrow back — a human performing rounding and rescaling manually,
which is exactly what Burxt exists to prevent.

| Operation | Scales | Rule |
|---|---|---|
| `+`, `-` | must match | unchanged |
| `*` | same | legal; contract still required (the scale doubles) |
| `*` | mixed | legal **only** with a rounding contract on the result |
| `/` | must match | unchanged — this decision covered `*` only |

Codegen shifts the exact product by `lhs_scale + rhs_scale - result_scale`,
which subsumes the same-scale case (`s + s - s = s`), so mixed and matching
scales share one path instead of being special-cased against each other. A
result *wider* than the exact product widens losslessly and rounds nothing.
The i128 intermediate reaches 10^36, so `pow10` now builds its constant from a
`u128` rather than one 64-bit word.

Tested to the numeric core's bar: exact ties at mixed scales in both modes and
both signs, checked against an independent exact-decimal implementation; the
scale-18 × scale-18 extreme where the intermediate needs 36 decimal places; the
widening direction; and a result that genuinely cannot fit a scaled i64, which
still traps loudly.

### v0.0.20: sum types and exhaustive matching (A6.0)

The milestone that makes a compiler expressible in Burxt — a `Token`, an
`Expr`, a `Type` are all sum types — and that delivers **exhaustiveness**, the
correctness-family feature committed long ago.

```text
enum Token { Plus, Number(Int), End }

fn describe(t: Token) -> Int {
    match t {
        Plus      => { return 1; }
        Number(n) => { return n; }
        End       => { return 0; }
    }
}
```

- **Construction is qualified, patterns are not.** `Token.Plus` needs its enum
  because nothing else says which type is meant; inside `match t {}` the
  scrutinee's type already says, so repeating it would be noise. The asymmetry
  is deliberate. A bare `Plus` is refused with advice naming the qualified form.
- **Every variant must be handled.** A missing variant is a compile error that
  *names the ones left out* — so adding a variant later turns every incomplete
  match into a list of what to fix, instead of a silent fall-through.
- **No `_` wildcard, and this is the load-bearing refusal.** A wildcard would
  silently absorb variants added later, which is exactly the guarantee
  exhaustive matching exists to provide. Refused with that reason, so the
  deferral is enforced rather than aspirational.
- **An exhaustive match whose arms all return IS a return.** The return-path
  prover learned this, by the same reasoning as an if/else where both branches
  return: exhaustiveness means the arms *are* all the paths.
- Layout is a tag plus an inline payload area, `{ i64, [N x i64] }`. Enums are
  aggregates, so the v0.0.12 ABI carries them unchanged — `byval` parameters,
  `sret` returns, value semantics — and `match` lowers to a `switch` whose
  default block is `unreachable`, because typeck already proved no tag is
  missing.

**The self-hosting consequence, and it is the useful finding here:** payloads
are scalars only, because an enum containing an enum has no finite size without
heap indirection. A lexer's `Token` is flat, so **the lexer is expressible
today** — but an AST node is recursive, so **the parser is M1-blocked.** That
sharpens the self-hosting path: the partial self-host can begin now and stops
precisely at the parser.

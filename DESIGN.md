# Burxt — Design Notes (v0.0.16)

**Burxt** is a typed, compiled programming language: exact decimals for money,
correctness by construction, native code through LLVM.

## Identity (the anchor)

> Burxt is **composition-first OOP with opt-in safe inheritance**, where the
> compiler rigorously enforces **objective** correctness (no null, no silent
> overflow, verified contracts, exhaustiveness) and makes SOLID design the
> **ergonomic default** — giving PHP's familiarity, Rust's safety, and a
> verification layer neither has.

Distinct from PHP (enforces nothing; null + inheritance footguns) and from
Rust (no inheritance, no built-in contracts). And honest: it does not claim to
mechanically enforce the unenforceable. Everything below serves this line.

## Thesis (what makes Burxt worth existing)

1. **Exact decimal is the DEFAULT numeric type for money.** No silent
   binary-float representation of currency. `Decimal<S, R>` carries scale and
   rounding contract in the type.
2. **Correctness by construction.** Rounding must be explicit; float↔decimal
   mixing is a compile error. (Refinement types come later.)
3. **Native, no runtime baggage.** Compiles through LLVM to a native binary.
   No VM, no GC (yet).

## Grammar principle

The grammar must be eloquent and easy to understand, without compromising the
thesis. Types read as plain English (`Decimal<2, RoundHalfEven>` = "two
decimal places, rounding half to even"), there is one obvious way to write
each construct, and every compile error reads like advice — it names the rule
and shows the syntax that fixes it. When brevity and clarity conflict,
clarity wins; exactness and explicitness are never traded for either.

## Semantics principles

Decided now, before the features that would test them are built:

### One equality, no coercion

`==` is the only equality Burxt will ever have, and it never converts.
Both sides must be the SAME type — no int→decimal promotion, no
scale rescaling, no truthiness, and never a second "looser" equality
operator. A comparison either compiles as an exact, value-level equality
or it is a compile error that says what to convert explicitly. This is
already the implemented behavior; it is now a standing rule for every
future type (struct field-wise equality, string byte equality, Option)
— they must all arrive as the SAME `==`, total within their type, or
be refused until they can.

### No null — absence is an explicit Option, handled or it doesn't compile

Burxt will never have null, nil, or sentinel values. When a value can
be absent, the type says so — `Option<T>` — and the compiler forces the
absence case to be handled before the `T` can be touched; there is no
"unwrap and hope". Consequences committed now:

- No feature may introduce a silently-absent value in the meantime: an
  operation that could fail to produce a value either takes the panic
  path (loud, like division by zero) or waits for Option.
- At the FFI boundary, a C null pointer must become `None` at the edge —
  a null must never travel one step into Burxt code as a "value".
- Option composes with the thesis: `Option<Decimal<2, RoundHalfEven>>`
  is "maybe money", and the rounding contract survives inside it.

## Problems Burxt solves by design

The well-known footguns of existing languages, and Burxt's stance on each —
honestly labeled: SHIPPED (enforced today), COMMITTED (decided, not yet
built), ASPIRATION (goal, no timeline), or OPEN TRADEOFF (a real fork in the
road, deliberately not pretended away). The unifying idea is the decimal
thesis generalized: **dangerous defaults become compile errors.**

### Shipped — enforced by the compiler today

- **Float money errors** (0.1 + 0.2 != 0.3): the founding thesis. No float
  exists; decimals are exact scaled integers with rounding contracts.
- **Silent integer overflow**: every + - * traps with a named runtime error
  (v0.0.5), never wraps. An overflowing balance is a disaster, not a wrap.
  Opt-in wrapping may come someday; wrapping-by-default never will.
- **Implicit conversions**: none, anywhere, ever. Int never becomes Decimal,
  scales never rescale, nothing is truthy.
- **One equality, no coercion** (see Semantics principles).
- **Uninitialized variables**: unrepresentable — `let` requires an
  initializer; there is no declaration without a value.
- **Mutable-by-default**: inverted. Immutable is the default; mutation is
  opt-in and visible (`let mut`, v0.0.4).
- **Shadowing / silent redefinition**: refused (v0.0.9) — a second
  `let x` is a compile error, not a quiet new variable.
- **Format-string and width bugs at the C boundary**: user bytes are never a
  format string; C's 32-bit int is a distinct `CInt` whose sign survives and
  whose range is checked (v0.0.9).
- **Error messages that teach**: every rejection names the rule and shows
  the syntax that fixes it. A design commitment since v0.0.2.

### Committed — decided now, built when their feature arrives

- **No null** — absence is `Option<T>`, handled or it doesn't compile (see
  Semantics principles).
- **Errors as values**: failures the program can handle will be typed
  values the compiler refuses to ignore — no invisible exceptions, no
  unchecked error codes. (Today the only failures are panics: loud + fatal.)
- **Exhaustiveness**: when case analysis on a closed type arrives (Option,
  enums, interface dispatch), every branch point must handle every case —
  adding a case later turns every incomplete match into a compile error.
- **Strings are UTF-8, bytes are bytes**: no implicit mixing, decided before
  a bytes type exists.
- **Money-specific defaults**: cross-currency arithmetic will require the
  currencies to be distinct types (nominal structs already give this shape);
  dates/timezones, when they come, arrive timezone-explicit or not at all.
- **Deterministic builds**: when Burxt grows dependencies, resolution is
  locked and reproducible from day one.

### Aspiration — the strongest differentiator, flagged without a timeline

- **Data races as compile errors.** A corrupted balance from two threads is
  the same disease as float money. This is genuinely hard (it is Rust's
  headline achievement); Burxt designs toward it (value semantics and
  immutability-by-default are the right substrate) but commits no date.

### Known interim measures — working, but not the final answer

Listed so they are not mistaken for finished work:

- **Compiler stack depth.** A 512 MB-stack thread holds ~30,000-node
  expression trees; deeper input aborts. The real fix is iterative AST
  walkers (see v0.0.11). Bounded by heap, not stack, is the goal.
- **Scale ceiling 18.** A scaled i64 carries at most 18 fractional digits.
  Arithmetic intermediates are already i128; widening the STORED
  representation is a separate, deliberate decision (it changes the ABI and
  the FFI story), not an oversight.
- **Register-pair aggregate returns: superseded by `sret` for now** (v0.0.12).
  A deliberate simplicity-and-uniformity choice over peak performance,
  reversible later as a pure optimization behind unchanged semantics.
- **Field reordering / padding minimization: not done, on purpose.**
  Declaration-order layout keeps offsets obvious and FFI predictable.
  Revisit only as an opt-in optimization, never as a silent default.
- **Array returns.** Array PARAMETERS work (v0.0.12); returning one needs
  whole-array binding at the call site, which is the copy question deferred
  with collections.
- **Method receivers pass as a plain pointer, not `byval`** (v0.0.14). Forced
  by vtable compatibility: a slot cannot name a concrete type, so it cannot
  carry `byval(T)`, and mixing the two lowerings produced silently wrong
  values. Sound because a non-mutating `self` is read-only. Ordinary aggregate
  parameters are unaffected.

### Open tradeoff — deliberately undecided, eyes open

- **Memory management.** GC (pauses — bad for "predictable"), ownership
  (no pauses, steep learning curve), ARC (middle ground) — every option
  costs something real. Burxt has deferred every allocation so far
  precisely so this fork is chosen once, deliberately. The safety-vs-
  ergonomics tension is permanent: the art is hiding strictness behind
  inference so code stays simple while the compiler stays strict.

## The OOP model — composition-first, opt-in safe inheritance (committed)

**This section supersedes the earlier "no implementation inheritance, ever"
stance.** Inheritance EXISTS, but constrained so the classic footguns
(fragile base class, diamond problem) cannot happen — which is the real goal
the earlier absolute rule was reaching for.

- Sharing **behavior** → traits (interfaces). Small by default.
- Reusing **state/structure** → composition ("has-a"). The ergonomic default.
- **Inheritance** ("is-a") only from a class explicitly marked `open`, and
  only single inheritance. Not marked `open` → cannot be extended, so the
  fragile-base-class problem is prevented by construction; no multiple
  inheritance, so no diamond.

```text
trait Printable {
    fn describe(self) -> String
}

// Composition is the natural default: Account HAS-A Ledger, not IS-A.
class Account : Printable {
    owner:   String
    balance: Decimal<2> = $0.00
    ledger:  Ledger                 // a field, not a parent

    fn describe(self) -> String {
        "Account of {self.owner}: {self.balance}"
    }
}

open class Shape { fn area(self) -> Decimal<4> }
class Circle : Shape { radius: Decimal<4> }   // allowed: Shape is `open`
// class Sneaky : Account { }                 // ERROR: Account is not `open`
```

The gap between PHP (inheritance-heavy) and Rust (no inheritance) is where
Burxt lives. Today's `struct` is the value-type substrate this grows from;
`class`, `trait`, `open` arrive with the aggregate ABI and dispatch.

### SOLID stance — enforce the objective, encourage the subjective

Claiming to "enforce all of SOLID" would overpromise: *single responsibility*
has no crisp definition, and hard-erroring on it would produce false
positives and lose trust. So:

| Principle | Burxt stance |
|---|---|
| Single Responsibility | Encouraged; optional lint. NOT a hard error — too subjective. |
| Open/Closed | Grammar-supported: `open` classes, traits extend without modification. |
| Liskov Substitution | Contract-checked: a subtype's `requires`/`ensures` may not violate the base's. |
| Interface Segregation | Structurally nudged: small traits are the easy path; lint warns on bloat. |
| Dependency Inversion | Depending on a trait is ergonomic; depending on a concrete class is the awkward opt-in. |

## Signature grammar — eloquent because it matches intent (committed)

95% familiar, 5% novel exactly where the thesis lives. Eloquence comes from
grammar matching the domain so closely that correct code reads like a
description of the problem.

### Contracts as first-class grammar

**This supersedes contracts-as-attributes below.** `requires` / `ensures` are
KEYWORDS, so a function reads as a self-documenting sentence:

```text
fn withdraw(acct: Account, amount: Decimal<2>)
    requires amount > $0.00
    ensures  acct.balance >= $0.00
{
    acct.balance = acct.balance - amount
}
```

Verified at compile time where possible, else a checked guard is inserted.
Either way the intent lives in the signature, not a comment. Attributes
remain for other cross-cutting metadata — they are not the contract syntax.

### Money and units as first-class literals

```text
let price = $19.99        // $ => Decimal<2>, no annotation needed
let tax   = 8.25%         // a real literal, not float/100
let dist  = 5.km
// let bad = price + dist    // ERROR: cannot add money to distance
// let bad = 5.usd + 3.eur   // ERROR: different currencies; convert explicitly
```

Units carry meaning in the type; illegal mixing is a compile error. This is
also how "comparisons across currencies" stops being a footgun.

### Pipelines and interpolation

```text
let owed = invoices |> filter(unpaid) |> map(amount) |> sum |> in_currency(usd)
print("Account of {owner}: {balance}")
```

Left-to-right reading instead of inside-out nesting; interpolation by default,
no concat or printf-juggling. Neither is novel (F#, Elixir, every modern
language) — both are proven, cheap eloquence.

### The permanent tension

Burxt wants both *many guarantees* and *Python-like ease*; these pull against
each other, since every guarantee adds ceremony. The craft is hiding
strictness behind good inference: **the compiler should be strict silently,
not loud.** Discipline: pick a FEW signature features (contracts, money/units,
composition-first OOP, the correctness family) and keep everything else
minimal and familiar. Resist the kitchen sink — a language novel on every
line gets admired and unused. When "more power" fights "still easy", bias to
easy plus inference.

## Attributes — cross-cutting metadata (committed)

Burxt will have **compile-time attributes**: `#[...]` metadata attached to
code that the COMPILER reads, checks, and discards — zero runtime trace, per
the no-runtime-baggage pillar.

Scope note: function/type CONTRACTS use the `requires`/`ensures` keyword
grammar above, not attributes — a contract is part of the signature, not
metadata about it. Attributes carry the genuinely cross-cutting rest
(deprecation, serialization, lint control, and type-level invariants that
have no signature to live in, e.g. `#[invariant(self.balance >= $0.00)]` on a
class).

Rules committed now:

- Attributes are parsed, typed, and validated language constructs — never
  comment-scraping (the pre-PHP-8 docblock hack is the cautionary tale).
  An unknown attribute is a compile error, not silently ignored metadata.
- Compile-time only. Runtime reflection is not core and may never come —
  it costs runtime baggage and the thesis doesn't need it.
- Discipline: attributes carry cross-cutting, machine-checked meaning —
  contracts first (`#[invariant]`, `#[requires]`, `#[ensures]`), possibly
  serialization/deprecation later. They are never a replacement for normal
  code; logic buried under decorator stacks is its own footgun.
- The `#` character is reserved in the lexer TODAY (with an error message
  saying what it's for), so no program ever uses it for anything else.

## Compiler architecture (backend-independent front end)

```text
Source (.bx)
  -> Lexer      (src/lexer.rs)      : text -> tokens
  -> Parser     (src/parser.rs)     : tokens -> AST (src/ast.rs)
  -> Typecheck  (src/typeck.rs)     : AST -> typed AST + errors
  -> Codegen    (src/codegen.rs)    : typed AST -> LLVM IR -> native object
  -> link       (cc)                : object -> executable
```

The front end (lexer/parser/typeck) knows NOTHING about LLVM. If we ever swap
to Cranelift or add an interpreter, only codegen.rs changes.

## Bootstrap plan

- Stage 0: this compiler, written in **Rust**, emitting via **LLVM 18**
  (inkwell).
- Stage 1 (future): rewrite the Burxt compiler in Burxt; compile it with
  stage 0. The day "Burxt compiles Burxt" = self-hosting = the language is
  real.

## Milestone log

### v0.0.1: the first vertical slice

The smallest program that proves the thesis: exact decimal arithmetic with a
declared scale, printed exactly. Integers supported too, as the simplest path
to "it runs". Decimals are represented as scaled i64 (value * 10^scale) —
exact, no float.

```text
let price: Decimal<2> = 19.99;
let qty: Int = 3;
let total: Decimal<2> = price * qty;
print(total);        // 59.97  — exact, never 59.970000000001
```

### v0.0.2: rounding contracts

A rounding contract is an optional second type argument:

```text
Decimal<2>                 // no contract: only exact arithmetic
Decimal<2, RoundHalfEven>  // ties to the even neighbor (banker's)
Decimal<2, RoundHalfUp>    // ties away from zero (commercial)
```

Rules:

- `+`, `-`, `* Int` are always exact, so they never require a contract.
- `Decimal * Decimal` and division produce digits beyond scale S, so they are
  compile errors unless the operands carry a contract saying how the result
  returns to S. `a*b` computes the exact double-scale product, then rounds.
- Operand types must match EXACTLY (scale and contract). `Decimal<2,
  RoundHalfEven>` and `Decimal<2>` never mix silently — Burxt does not pick.
- Literals never round: `let x: Decimal<2, RoundHalfUp> = 1.999;` is still
  refused. The contract governs arithmetic, not silent literal truncation.
- `Int / Int` is refused for now: truncation is silent rounding. It will
  return with explicit semantics.
- Codegen: each mode becomes one tiny generated IR function
  (`@burxt.round.<mode>`: sdiv/srem + tie adjustment), called where needed.
- Division by zero and i64 overflow were unchecked here; both became named
  runtime errors in v0.0.5.

### v0.0.3: functions, control flow, Bool

Burxt is now a real programming language: recursion makes it computationally
complete without needing mutation yet.

```text
fn total(price: Decimal<2>, qty: Int) -> Decimal<2> {
    return price * qty;
}
print(total(19.99, 3));    // 59.97
```

Rules:

- Every function declares parameter types and a return type, and the
  typechecker PROVES it returns on every path (last statement is a `return`,
  or an if/else where both branches return). Code after a returning statement
  is an error, not a warning.
- Functions are hoisted: define them in any order, call them mutually.
- Argument and return types must match exactly — same rules as `let`. A
  literal argument adopts the parameter's type (`total(19.99, 3)` works).
- `if cond { } else if { } else { }`: the condition must be a Bool, braces
  are required, blocks are real lexical scopes.
- Comparisons (`== != < <= > >=`) are always exact (scaled integers compare
  directly) and produce a `Bool`. Both sides must have the SAME type —
  comparing Decimal<2> to Decimal<3> is refused like adding them would be.
  A literal adopts the type of what it faces: `balance > 0.00` just works.
  Comparisons do not chain (`a < b < c` is not an expression).
- `Bool` is first-class; it has `==`/`!=` but no order.
- Codegen: all values are i64 (Bool holds 0/1, i1 only at branches); user
  functions are mangled `bx.<name>` so they can never collide with libc.

### v0.0.4: mutation and loops

Immutable is the default; mutation is opt-in and visible at the declaration:

```text
let mut b: Decimal<2, RoundHalfEven> = 1000.00;
let mut m: Int = 0;
while m < 12 {
    b = b * 1.01;      // contract applies at every step
    m = m + 1;
}
```

Rules:

- `name = value;` only compiles for a `let mut` binding, and the value's type
  must match the declaration exactly. Parameters are immutable.
- `while` needs a Bool condition and braces, like `if`. A loop body never
  counts as "returns on every path" (the condition may be false at entry).
- Codegen: every alloca goes in the function's entry block, so a `let`
  inside a loop body cannot grow the stack per iteration.

### v0.0.5: checked arithmetic — no silently wrong numbers, ever

Every `+`, `-`, `*` (including the internal double-scale products behind
Decimal*Decimal and division) goes through `@burxt.checked.<op>`, built on
LLVM's `llvm.s{add,sub,mul}.with.overflow` intrinsics. On overflow the
program prints

```text
burxt runtime error: arithmetic overflow — the exact result no longer
fits in the value range
```

to stderr and exits with code 70. Division by zero (and the lone
i64::MIN / -1 quotient) gets the same treatment — a named error instead of a
raw SIGFPE. This closes the last "silently wrong number" hole in the i64
representation; a wider representation can come later, but wraparound was
never acceptable.

### v0.0.6: FFI — call into C

The key unlock for the platform roadmap: every platform API Burxt will ever
touch goes through this door.

```text
extern fn llabs(x: Int) -> Int;
print(llabs(0 - 42));    // 42
```

Rules:

- `extern fn name(params) -> ret;` declares a C function. The name is the
  real linker symbol — never mangled (user fns keep their `bx.` prefix, so
  the two can never collide). Matching the C side's actual signature is the
  programmer's contract, as in every FFI.
- Only `Int` crosses the boundary for now. C has no Decimal — passing the
  raw scaled i64 would silently shed its scale and rounding contract, the
  exact meaning-loss Burxt exists to refuse. Strings and richer types widen
  this deliberately in A4.
- `printf`, `fputs`, and `exit` are reserved (the Burxt runtime declares
  them itself); call them through a differently-named C wrapper.
- Extern calls typecheck exactly like ordinary calls — same arity and type
  errors, no special cases.

### v0.0.7: strings — literals only, no heap, no lies

A String is a pointer to an immutable, NUL-terminated byte array. In this
slice every String is a literal living in .rodata, so there is NO allocation
— and therefore no free/GC question to answer dishonestly. Operations that
need allocation are loud, advice-style refusals until the allocation story
exists:

- `+` (concatenation): "needs memory allocation — coming with collections".
- `==`/ordering: "needs a byte-equality runtime helper, coming with
  collections".

What works: literals with exactly four escapes (`\\ \" \n \t` — no `\0`, so
interior NULs are unrepresentable), `print`, `let` / `let mut` rebinding,
user-fn params and returns (every value is 'static by construction), and the
FFI widening: an extern String parameter passes a borrowed, read-only
`const char*`. Extern returns stay Int-only — Burxt cannot yet track who
owns memory a C function returns.

Codegen grew up for this: `gen_expr` now returns `BasicValueEnum`, variable
slots and function signatures are typed per Burxt type, and String is LLVM's
opaque `ptr` — never an integer, so pointer width stays the target's
business (wasm32-safe). User bytes are always a printf ARGUMENT, never the
format string.

### v0.0.8: structs — the OOP substrate

```text
struct LineItem {
    price: Decimal<2>,
    qty: Int,
}
let mut item: LineItem = LineItem { qty: 3, price: 19.99 };
print(item.price * item.qty);   // 59.97
item.price = 12.50;
```

Rules:

- **Nominal typing.** Two structs with identical fields are different types —
  the name is a contract, exactly as Decimal<2> and Decimal<2, RoundHalfEven>
  are kept apart. An `Invoice{total}` never passes where a `Refund{total}` is
  expected.
- Construction names EVERY field (any order — the names carry the meaning);
  Burxt does not invent defaults. Field values obey the same exact-match and
  literal-adoption rules as `let`.
- **Value semantics.** Assignment copies the whole struct — no hidden
  aliasing, no GC pressure. Nesting is allowed; the copy is naturally deep
  because the layout is flat. A struct cannot contain itself (no finite size).
- Field assignment (`item.price = ...`, nested paths too) requires the
  BINDING to be `let mut`. Mutability is per-binding, not per-field.
- Struct literals are not allowed directly in an if/while condition (the `{`
  must start the block); parenthesize if ever needed.
- Deferred, each with a reason: struct fn params/returns (by-pointer ABI,
  next milestone), `==` (field-wise semantics undecided), `print(struct)`
  (display story), struct FFI (C-layout contract).
- Codegen: real LLVM struct types (`bx.<Name>`), aggregate values built with
  insert_value, field reads via extract_value, field writes via struct GEPs.

### The OOP direction: SOLID by construction

Burxt is OOP by default, and the object model is decided now (keywords
`interface`, `is`, `self` are already reserved):

- **Methods** are receiver functions — `fn (self: LineItem) total() ->
  Decimal<2>` — plain functions in the receiver's namespace, no hidden
  `this`. Mutating methods say so: `fn (mut self: Account) deposit(...)`,
  callable only on `let mut` bindings.
- **Interfaces** are declared contracts: `struct LineItem is Priceable`.
  Conformance is never inferred from shape — structural satisfaction is
  silent conformance. The check is exact: every method, exact signature.
- **Inheritance: superseded — see "The OOP model" above.** This section
  originally said "no implementation inheritance, ever". The current
  direction keeps the goal (no fragile base class, no diamond, no hidden
  override dispatch) but reaches it with `open`-only single inheritance
  rather than prohibition, so genuine is-a modeling is available and
  composition stays the default.
- Dispatch will be dictionary-passing (fat pointers): the method table
  lives OUTSIDE the struct, so struct layout never changes and stays
  FFI-viable. Static dispatch whenever the concrete type is known.
- SOLID mapping: S — cheap nominal structs; O — extend by new type + `is`;
  L — exact conformance, no overriding; I — small interfaces (exact
  conformance keeps them small); D — functions take interface-typed
  parameters, depending on contracts, not concrete structs.

### v0.0.9: hardening — findings from the adversarial review

An agent review of the strings release confirmed one serious bug and three
sharp edges; all fixed:

- **`CInt`** (the serious one): extern `-> Int` mapped C's 32-bit `int` to
  i64, so `strcmp` returned its sign in undefined upper bits — every
  `strcmp(...) < 0` took the wrong branch. C's int is now a distinct FFI
  type: `extern fn strcmp(a: String, b: String) -> CInt;`. Returns
  sign-extend; arguments range-check at runtime (a value that doesn't fit a
  C int is a loud exit-70 error, never a silent wrap). CInt exists ONLY in
  extern signatures; Burxt code sees Int.
- A raw NUL byte in a string literal (as opposed to the already-refused
  `\0` escape) silently truncated the string at codegen. Now a lexer error —
  "interior NULs are unrepresentable" is true again.
- Decimal scales above 18 panicked the COMPILER (10^19 > i64). Scale is now
  capped at 18 with an advice error, literal fractional digits likewise, and
  the internal rescaling powers are overflow-checked.
- Reserved extern symbols now include `main` and `stderr` (the runtime emits
  both); colliding declarations are compile errors instead of link failures.

Plus one language rule from the design review: **shadowing is refused** —
a second `let x` is an error naming the first declaration, not a quiet new
variable.

### v0.0.10: arrays — fixed-size, always bounds-checked

```text
let splits: [Decimal<2>; 3] = [10.00, 5.99, 4.01];
let mut sum: Decimal<2> = 0.00;
let mut i: Int = 0;
while i < len(splits) { sum = sum + splits[i]; i = i + 1; }
print(sum == 20.00);   // true — A5's refinement, checked by hand for now
```

Rules:

- `[T; N]` is a fixed-size stack array, N in 1..=65536 (a huge N would be a
  silent SIGSEGV — the one death Burxt never permits). Elements are scalars
  (Int, Bool, Decimal) for now. Growable vectors wait for the allocation
  story, decided once.
- **Bounds are checked on every access, always.** A computed index out of
  range dies with a message that names the offending index and the valid
  range (exit 70). A LITERAL index that is provably out of range is refused
  at compile time — it would always fail, so it fails now.
- Element writes (`a[i] = v`) follow the binding's `let mut`, like fields.
- `len(a)` folds to the constant N at compile time — zero runtime cost,
  and code stays honest when the length changes.
- Arrays exist only behind bindings in this slice: created by a literal in
  `let`, touched via `a[i]` and `len(a)`. Bare `a` in an expression,
  `print(a)`, whole-array assignment, fn params/returns, struct fields —
  each refused with the reason and its arrival milestone.

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

## Testing

`cargo test` runs a data-driven suite:

- tests/pass/NAME.bx + NAME.stdout — must compile & run with exactly that
  output.
- tests/fail/NAME.bx + NAME.stderr — must be rejected with an error
  containing that text.
- tests/panic/NAME.bx + NAME.stderr — must compile, but die at runtime with
  a nonzero exit and that text on stderr.

Adding a test = dropping two files in the right directory.

## Roadmap: write once, run native everywhere

Burxt is committed — as of now, so the architecture never needs retrofitting —
to running natively on web, desktop, and mobile. This is reachable because
every target reduces to one of two output paths from the same LLVM backend:

1. **Native code** — desktop Linux/macOS/Windows, Android, iOS. Different
   CPU + libc + object-format combos that LLVM already handles.
2. **WebAssembly** — the web (and edge/server via WASI).

So platform reach is not five rewrites. It is (a) making the backend
target-parameterized (`burxt build --target <triple>`) and (b) handling each
platform's linking/packaging — affecting only codegen plus a thin packaging
layer, never the front end.

Target triples to support:

- x86_64 / aarch64 Linux
- x86_64 / aarch64 macOS (darwin)
- x86_64 Windows (msvc)
- aarch64-linux-android
- aarch64-apple-ios
- wasm32-unknown-unknown (web)
- wasm32-wasi (edge/server)

### Sequence: capability BEFORE reach

A cross-platform print is worthless. The language becomes real first, then
it travels.

#### Phase A — real language (Linux only)

- A1. Rounding contracts `Decimal<Scale, Rounding>` — DONE (v0.0.2)
- A2. Functions + control flow — DONE (v0.0.3, v0.0.4; checked arithmetic
  v0.0.5)
- A3. FFI / call-into-C — THE KEY UNLOCK: how any Burxt program reaches
  platform APIs on every target. — DONE for Int signatures (v0.0.6);
  String params (v0.0.7); widens further with A4's types.
- A4. Strings (v0.0.7), structs (v0.0.8), arrays (v0.0.10) — DONE
- A4.5. The aggregate ABI: `byval` params, `sret` returns, layout guarantee
  — DONE (v0.0.12)
- A4.6. Composition-first OOP: receiver methods (v0.0.13), traits + `dyn`
  dispatch (v0.0.14) — DONE. `class` and `open` single inheritance still to
  come (see "The OOP model") <- IN PROGRESS
- A4.7. Signature grammar: money/unit literals (`$19.99`, `8.25%`, `5.km`),
  string interpolation, pipelines
- A4+. OOP by default, SOLID-aligned: by-pointer ABI + receiver methods,
  then interfaces as behavioral contracts (dictionary dispatch). No
  implementation inheritance — a type satisfies an interface exactly or it
  is a compile error, so Liskov violations are unrepresentable.
- A5. Contracts + refinement types: `requires`/`ensures` keywords,
  "balance >= 0", "splits sum to total", Liskov-checked overrides; then
  SOLID ergonomics and lints

#### Phase B — cross-compilation and desktop

- B1. `burxt build --target <triple>`
- B2/B3. Desktop matrix first: Linux, macOS, Windows

#### Phase C — mobile

- Android: NDK, .so + thin Kotlin/JNI app shell
- iOS: Mach-O, Xcode signing

#### Phase D — web

- wasm32 + JS host glue; then wasm32-wasi for edge/server

#### Ongoing

- Self-hosting: the day Burxt compiles Burxt, the language is real.

### Design rules (platform)

- The front end NEVER assumes a platform. All platform differences live
  behind the target triple + the packaging layer.
- I/O and platform APIs go through FFI — never hardcoded into the language.
- Exact-decimal semantics must be byte-identical on every target. The
  scaled-integer representation makes this free: no float means no per-CPU
  divergence.

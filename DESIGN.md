# Burxt — Design Notes (v0.0.56)

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
- **Data races as compile errors** (promoted from aspiration, 2026-07-25).
  The mechanism is decided: **region ownership** — a region has exactly one
  owner, so everything inside it is reachable by one thread and a race cannot
  be expressed. No per-object borrow checking, no collector, no refcounts.
  See `spec/M1-MEMORY-MODEL.md`.

### Aspiration — flagged without a timeline

- *(Data races as compile errors moved to COMMITTED — see below.)*
- **Full static verification of contracts.** `requires`/`ensures` are checked
  at runtime today; proving them at compile time is SMT-solver territory and
  gets its own phase.

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
- **Stack overflow is the one failure Burxt does not name.** Measured at
  v0.0.23: recursion 100,000 deep works; 1,000,000 deep dies with a raw
  SIGSEGV (exit 139), not a named error and not exit 70. **`return tail`
  (v0.0.29) gives a way to AVOID it, which is not the same as naming it** — an
  unmarked deep recursion still dies anonymously. Every other failure
  mode — array bounds, integer overflow, division by zero, region exhaustion —
  reports itself and exits 70. Honest severity: this is LOUD, so it is a
  diagnostics gap rather than a correctness hole like a wrong number would be.
  Fix is either a guard-page signal handler (what Rust does) or a depth counter
  in codegen. Worth doing so nothing in the language fails anonymously.
- ~~**Tail calls are not optimized**~~ — **shipped in v0.0.29** as
  `return tail f(...)`: a *guaranteed, checked* tail call, never an invisible
  optimization. Measured: 50,000,000 frames deep in constant stack; the same
  program without `tail` dies. What remains deferred is the *inferred* case —
  Burxt will not silently turn an unmarked tail call into a loop, because then a
  small edit could silently reintroduce stack growth. That is the whole point of
  making it explicit.
- **Multiplication scale rule refined** (v0.0.19), superseding "operands of
  `*` must have matching scales": `*` permits mixed operand scales when the
  result binding supplies a rounding contract; `+`/`-` remain strict; `/` is
  untouched. Rationale: multiplication combines a quantity with a rate (differing
  scales are natural), addition combines like quantities (differing scales are
  suspicious). The mandatory contract preserves no-silent-rounding, and must
  never become optional.
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

## The OOP model — composition only (DECIDED, v0.0.46)

> **Decision taken (v0.0.46): `class` and `open` single inheritance are DROPPED.**
> Composition-only is final, and this is now the settled model rather than a
> waypoint.
>
> The reason is evidence, not taste. Traits + `impl` + composition shipped in
> v0.0.13–v0.0.14, and in every version since — regions, sum types, contracts,
> conservation laws, a self-hosted lexer and parser — **nothing has needed
> inheritance.** Not once. An item that sits on a roadmap through thirty versions
> without a single program asking for it is not "planned", it is a wish, and this
> project's rule is that a feature earns its place by being needed.
>
> What the earlier plan was reaching for, it already has: *reuse* comes from
> composition, *substitutability* from traits, and the fragile-base-class and
> diamond problems are absent because there is no base class to be fragile. The
> "opt-in safe inheritance" design below is kept as the record of what was
> considered and why it was dropped.

**Superseded plan (kept for the record).** Inheritance would have existed, but
constrained so the classic footguns (fragile base class, diamond problem) could not
happen — which is the real goal the original absolute rule was reaching for.

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

The gap between PHP (inheritance-heavy) and Rust (no inheritance) is where Burxt was
going to live. **In the event it lives at the Rust end**, and got there by finding
that nothing needed the other half.

### SOLID stance — enforce the objective, encourage the subjective

Claiming to "enforce all of SOLID" would overpromise: *single responsibility*
has no crisp definition, and hard-erroring on it would produce false
positives and lose trust. So:

| Principle | Burxt stance |
|---|---|
| Single Responsibility | Encouraged; optional lint. NOT a hard error — too subjective. |
| Open/Closed | Traits extend behaviour without modifying what exists. (No `open` classes — see the decision above.) |
| Liskov Substitution | Unrepresentable to violate: a type satisfies a trait exactly or it is a compile error, and there is no subtype to weaken a contract. Contracts themselves are checked (v0.0.43). |
| Interface Segregation | Structurally nudged: small traits are the easy path; lint warns on bloat. |
| Dependency Inversion | Depending on a trait is ergonomic (`dyn Trait` as a parameter); depending on a concrete type is the awkward opt-in. |

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
- **Inheritance: none, and that is settled (v0.0.46).** This section originally
  said "no implementation inheritance, ever"; a later revision softened it to
  `open`-only single inheritance; the decision above closes it back on the original
  answer, this time with evidence rather than conviction — thirty versions of real
  programs never needed it. The goals it was reaching for (no fragile base class, no
  diamond, no hidden override dispatch) are met by not having the mechanism at all.
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

### v0.0.21: string bytes, and the first self-hosted piece

```text
print(byte_at("AbZ", 1));    // 98
```

`byte_at(s, i)` reads the i-th byte as an Int, bounds-checked with a message
naming bytes. It is **named for bytes deliberately**: A4.4 refused a bare
`s[i]` precisely because it would hide whether an index means a byte or a
character, and a builtin whose name says "byte" cannot hide it. Bytes
zero-extend, so a UTF-8 continuation byte comes back as 195, never negative.

**A Burxt lexer now exists, written in Burxt** — `examples/lexer.bx`, and a
test pins its output. It tokenizes real Burxt-ish source into `Plus`, `Number`,
`Name`, and friends, and it needs **no heap at all**:

- a token referring to source text carries a `(start, length)` **span**, not an
  owned substring — so no allocation;
- numeric literals are **accumulated arithmetically** as digits arrive
  (`value * 10 + (byte - 48)`), so no string building.

That is why the lexer runs before the memory model exists, and it is the
concrete first step of the self-hosting path. `match` earns its keep here: add
a variant to `Token` and the printer stops compiling until the case is handled.

**One compiler bug this found**, which is the value of writing real programs:
a struct field holding an enum panicked the compiler, because struct bodies
were filled in before enum types existed. Enums are now created first — a
total order, not a guess, since enum payloads are scalars and so can never
reference a struct. That fix is what lets the lexer return "the token, and
where to continue" as one `Scan` value.

### v0.0.22: the parser self-hosts — and the memory model was not the blocker

**`examples/parser.bx` is a Burxt expression parser and evaluator, written in
Burxt.** `1.00 + 2.00 * 3.00 = 7.00`, with correct precedence, parentheses, and
exact decimals — every result checked against an independent exact-decimal
implementation.

**The correction that matters:** v0.0.20 recorded that the parser was
M1-blocked, because an AST node is a recursive enum. That was wrong, and it was
wrong in an instructive way. **An AST does not need recursive types.** Nodes
live in a flat **arena** and refer to their children by **index**, which is how
Zig and Carbon build theirs. No recursion in the type, no heap, no memory
model. The parser was blocked on believing it was blocked.

What it actually needed were three restrictions lifted, none of them semantic —
all three were conservatism written early, not consequences of the design:

- **Arrays may hold aggregates.** A `[Node; 64]` is stack-allocatable; the old
  "elements must be Int, Bool or Decimal" was arbitrary. Nested arrays stay
  refused, with a reason: `a[i][j]` could not be written.
- **Structs may hold arrays.** The restriction's own message said "coming with
  the aggregate ABI" — which shipped in v0.0.12, so it was simply stale.
- **Indexing applies to any place, not just a bare name.** `self.nodes[i]`
  now reads and writes, via one `gen_place_addr` walker shared by both. This
  replaced a half-feature: an indexed *write* through a field path had briefly
  existed with no matching *read*.

Crucially, **no new semantics were added.** The arena mutates through a
`mut self` method — the by-reference receiver from v0.0.13 — so value semantics
stand untouched. Mutable aggregate *parameters* would have been a second
exception to A4.5's value-copy principle, and were deliberately not added.

**What self-hosting still needs from M1:** growable storage. The arena is a
fixed `[Node; 64]`, so a real compiler needs either a larger fixed budget or
heap growth. That is a genuine M1 dependency — but it is now a question of
*scale*, not of *expressibility*, which is a far smaller wall than the one
recorded two versions ago.

### v0.0.23: regions — M1 slice 1

```text
region tx {
    let inner: Int = outer + 1;
    print(inner);
}   // everything allocated here released in O(1)
```

The first slice of the memory model decided in `spec/M1-MEMORY-MODEL.md`:
**regions as the unit of ownership.** Opening a region records where the bump
cursor stands; closing it resets the cursor. That reset *is* the deallocation —
no per-object free, no refcount, no collector, no scheduler. The
no-runtime-baggage pillar holds without reinterpretation, because a pointer
that moves forward is not a runtime.

Region memory exhaustion is a named runtime error, not a silent overrun,
holding the same standard as every other check.

Refused with reasons, per the spec's must-NOT list: nested regions (one level
for now), and a region whose name collides with a variable.

**Two staging corrections the build immediately exposed**, both recorded in the
spec rather than worked around:

- **`List<T>` as specified needs generics, which Burxt deliberately does not
  have.** So the next slice is **built-in growable arrays** — a dynamic `[T]`
  beside the fixed `[T; N]`, element type from the annotation — not a generic
  library type. Go's slices are built in for exactly this reason.
- **Escape checking cannot come after the first allocation.** The spec had it
  as a later slice, but a region-allocated value that escapes is a
  use-after-free — the silently-wrong behaviour Burxt refuses everywhere. So it
  ships in the *same commit* as the first thing that allocates. "We will add
  the check next" is not a standard this project applies to anything else.

### v0.0.24: growable arrays + escape checking — M1 slice 2

```text
region parse {
    let mut nodes: [Node] = [];
    push(nodes, n);          // grows in the region
    print(len(nodes));
}                            // all of it released in O(1)
```

`[T]` is a growable array living in the enclosing region — distinct from the
fixed, stack-resident `[T; N]`. **No generics involved:** the element type comes
from the annotation, exactly as Go's slices are built in rather than generic.
Represented as `{ data, len, cap }`; `push` doubles capacity in the region when
full; indexing bounds-checks against the RUNTIME length.

**Escape checking ships in the same commit**, because allocation without it
would be a use-after-free. Two rules turn out to be sufficient:

1. **A region-allocated value may only be bound inside a region.** Since block
   bindings already do not escape their block, this single rule removes every
   assignment route out — there is nowhere outside to put it.
2. **A function may not return a region-allocated type.** That is the only other
   way the value could outlive the region its caller opened.

Taint propagates: a struct with a `[T]` field is itself region-allocated, so
`Holder { xs: [] }` outside a region is refused too. Both rules name the fix.

**The arena pattern self-hosting needs now works**: a struct holding growable
storage, mutated through a `mut self` method, inside one region — verified at
500 nodes, where the parser was previously capped at a fixed 64.

**The arena tradeoff, paid visibly:** growing copies into a fresh block and
abandons the old one, because a bump allocator cannot free an individual
object. That space returns when the region ends. Documented in the codegen
rather than hidden, since it is a real cost of the model.

### v0.0.25: string concatenation — M1 slice 3

```text
region r {
    let greeting: String = "Hello, " + name + "!";
    print(len(greeting));
}
```

`+` on String joins into the enclosing region, retiring the oldest entry on the
ownership ledger. The result is NUL-terminated, so a joined string is still a
plain `const char*` at the FFI boundary — indistinguishable from a literal, and
a test passes one to C's `strlen` to prove it. Byte equality works across the
two, since `==` was always about bytes rather than pointers.

Escape checking needed one addition, and the reason is worth recording: a
concatenated String lives in a region while a literal lives in `.rodata`, and
**both have type `String`** — so the type alone cannot say whether a value
escapes. The check therefore inspects the *expression*: `expr_allocates` walks
the tree, and returning anything it flags is refused.

**A reclassification found while building this:** interpolation-as-a-value was
recorded as M1-blocked, but it is not. It needs a number-to-string formatter
writing into memory — new machinery, not an ownership question. It is no longer
an M1 ledger entry; it becomes its own small slice once a formatter exists.

### v0.0.26: storable trait objects — M1 slice 4, and a corrected claim

```text
struct Holder { item: dyn Priced, label: Int }
let h: Holder = Holder { item: book, label: 1 };   // previously refused
print(h.item.price());
```

**A struct field may now hold a trait object.** The old refusal said a struct
"may outlive" what the object borrows — but when both are scoped to the same
block, it cannot. Block scoping was already doing the work; the refusal was
broader than the reason behind it.

This also fixed a real gap: **the concrete-to-`dyn` coercion only happened in
`let`**, so `Holder { item: book }` failed even though the equivalent binding
worked. The coercion now lives in `check_expr`, where every site that knows its
expected type passes through — struct fields, call arguments, returns — instead
of being special-cased in one place.

**A claim I got wrong, corrected here.** The M1 spec listed returnable and
storable `dyn` as things regions would unblock. Storable: yes. **Returnable:
no, and regions were never going to help.** A `dyn` borrows its *source
binding*, which is an ordinary stack local — so returning one dangles whether
or not a region is involved. Regions bound the lifetime of *region-allocated*
data; they do not change what a trait object points at. I briefly marked `dyn`
as region-allocated to force it, which broke every existing `dyn` test and was
the right kind of failure: the tests caught a category error.

So the remaining two ledger entries are re-diagnosed rather than retired:

- **Returning a `dyn`** — needs borrow tracking, not memory. Regions cannot fix
  it.
- **Mutating methods through a `dyn`** — needs to know the value behind the
  object was declared mutable. Regions bound its *lifetime*, not its
  *mutability*. The error now says exactly that.

### v0.0.27: the self-hosted parser is uncapped — M1 complete

```text
region parse {
    let mut a: Arena = Arena { nodes: [], count: 0, pos: 0, last: -1 };
    let root: Int = a.expr(src);
    print("{src} = {a.eval(root)}");
}
```

`examples/parser.bx` now uses `[Node]` instead of `[Node; 64]`, so **no node
budget is declared anywhere.** Verified on a 300-term expression: 599 nodes, all
allocated in one region and released together. That is what the memory model was
for.

**A link-time bug this found**, worth recording because it is a repeat of a
class already seen: two helpers each declared libc `fprintf`, so LLVM renamed
the second and the program failed to link against `fprintf.4`. Same collision
class as the reserved `main`/`stderr` symbols. There is now a single
get-or-declare helper, which is the general fix rather than a patch — any
runtime symbol declared in more than one place will do this.

**M1 is complete.** All four slices shipped: regions with a bump allocator,
growable arrays with escape checking, string concatenation, and storable trait
objects. Two of the spec's predictions were corrected along the way rather than
forced to come true (interpolation-as-a-value was never memory-blocked;
returnable `dyn` was never going to be fixed by regions), and both corrections
are recorded in the spec they came from.

### v0.0.28: reading a file, and rendering a value

```text
region source {
    let text: String = read_file("examples/sample.bx");
    print("--- {len(text)} bytes read");
    let n: Int = tokenize(text);
}
```

Two builtins, chosen because they were the two things a Burxt-hosted compiler
literally could not do: **it could not read its own input, and it could not build
an error message.**

- **`read_file(path) -> String`** reads a whole file into the current region,
  NUL-terminated, so it is an ordinary Burxt String afterwards. A file that
  cannot be opened is a *named* runtime error, not a silent empty string — the
  same standard bounds checks and overflow already meet. Why a builtin rather
  than FFI: `extern fn` returns are Int/CInt only, because a C function that
  returns a pointer returns memory belonging to nobody. `read_file` allocates in
  a region the compiler can see, so ownership stays answerable.
- **`to_string(v) -> String`** renders Int, Bool and Decimal into region storage
  using the *same format strings the printer uses* — one formatter, so a printed
  value and a rendered one can never disagree. `Bool` allocates nothing (both
  spellings are literals) and therefore needs no region. `to_string` on a String
  is refused: it would only copy it.

**And that retired the oldest entry on the ledger.** Interpolation-as-a-value
was reclassified at v0.0.25 as needing a formatter rather than memory. The
formatter now exists, so `let s: String = "n is {n}"` compiles — and it
**desugars to `to_string` + `+`** rather than getting a lowering of its own. A
test asserts the interpolation is byte-equal to the hand-written join, which is
the property the desugaring buys: they are the same program by construction.
`print("...{x}")` keeps its no-allocation path and still needs no region, so
nothing that used to compile got slower or stricter.

Escape checking needed no new rule — `expr_allocates` already flagged `+` on
Strings, so an interpolated value cannot outlive its region for the same reason
a concatenated one cannot.

**A repo hygiene fix shipped here too:** eight compiled example/test
executables had been committed. They are untracked now, with `.gitignore`
covering the bare, extensionless outputs `burxt build` writes into the working
directory.

### v0.0.29: guaranteed tail calls, and two region bugs found on the way

```text
fn count_down(n: Int, acc: Int) -> Int {
    if n <= 0 { return acc; }
    return tail count_down(n - 1, acc + 1);   // constant stack, or it will not compile
}
print(count_down(50000000, 0));               // 50 million frames deep
```

**`return tail f(...)` is a checked guarantee, not an optimization.** It lowers
to LLVM `musttail`, which *fails the build* if the call is not genuinely in tail
position — so there is never a silent difference between "optimized" and "hoped
for". The same program without `tail` dies at that depth, and a test asserts the
IR contains exactly one `musttail`, on the call that asked for it and nowhere
else. The guarantee is explicit by design: inferring it would mean a small edit
could silently reintroduce stack growth, which is the failure mode the whole
feature exists to remove. This is NOVELTY §4, and the same shape as every other
promise in the language — declare the intent, and the compiler guarantees it or
refuses with a reason.

`musttail` is only legal when the caller's and callee's **prototypes match**, so
that condition is checked in Burxt's own words rather than surfaced as an LLVM
verifier message:

```text
a guaranteed tail call reuses this frame, so `step` and `helper` must have the
SAME signature — `step` takes (Int) -> Int, but `helper` takes (Int, Int) -> Int.
```

Self-recursion satisfies that trivially, and mutual recursion does when the
signatures agree — which covers the loop use case. Also refused, each with its
own reason: a tail call into an `extern fn` (the C side owns that ABI, and
Burxt's width conversion has to happen *after* the call returns), aggregates
passed or returned by hidden pointer, and `return tail` on something that is not
a call. `tail` is now a keyword, so a program that used it as a name gets a
compile error rather than a changed meaning — the v0.0.17 syntax-change law.

**One refusal is a soundness rule rather than a limitation:** `return tail`
cannot leave a `region`. A region is released on the way out, but a tail call
never comes back to do it — and the release would have to happen *before* the
call, while the arguments may still point into the region.

**And that question exposed two real bugs in regions, both fixed here:**

- **A `return` from inside a region leaked it.** The cursor was only rewound at
  the closing brace, so leaving early skipped the release and the bump pointer
  climbed for the life of the process. A function that returned from inside a
  region leaked its region *on every call*. Now `return` releases the region on
  the way out, computing the result first (the expression may still be reading
  region storage). The regression test calls such a function 30,000 times, which
  would otherwise die of region exhaustion.
- **The return-path prover did not know a region body can return.** It demanded
  a second `return` after the block and then called that statement unreachable —
  there was no way to write a function that returns from inside a region at all.
  Before the fix the combination emitted invalid IR; a region is a lexical scope,
  not a branch, so if its body returns on every path, so does the region.

Worth stating plainly: **the tail-call work is what surfaced both.** Asking
"what has to happen between the call and the `ret`?" is the same question as
"what has to happen between the last statement and the `ret`?", and the second
one had two wrong answers.

### v0.0.30: exactness that survives the boundary (NOVELTY §1, slice 1)

```text
extern fn record_cents(amount: Decimal<2> as scaled) -> Int;

print(record_cents($19.99));   // C receives 1999 — exact, by declaration
```

Until now a Decimal simply could not cross into C. That was safe, but **"Decimals
cannot cross" is a missing feature; "Decimals cross only through an encoding that
cannot lose them" is a guarantee.** This slice converts the first into the second,
and the difference is the whole point of NOVELTY §1: real financial defects
overwhelmingly live at boundaries, not in arithmetic, and every language guards
the arithmetic and then abandons the wire.

**`CDouble`, an FFI-only type that models C's `double` honestly** — the same move
`CInt` made for C's `int`. It exists so a lossy crossing can be *named*, and
therefore refused. Without a name for the foreign type, "a Decimal may not bind
to a float" is unspellable, so the guarantee cannot be checked; it is merely
absent. Burxt still has no float type of its own and this is not one.

- **`Decimal<S>` → `CDouble` is a compile error, always**, with no flag and no
  escape. The message names the concrete loss and both exact alternatives:
  *"a C `double` cannot hold Decimal<2> exactly — a value like 0.10 is not
  representable in binary floating point, so this crossing would silently change
  the amount."*
- **`Int` → `CDouble` is allowed but range-checked at runtime.** A double holds
  every integer up to 2^53 exactly and starts skipping them after that, so
  `|n| > 2^53` is a named error with exit 70. Handing C a different integer than
  the one written is the same class of defect as a silent rounding.
- **A `CDouble` return stays refused.** Burxt has no exact way to receive a real
  number, and inventing an inexact receiver to complete the matrix would
  contradict the thesis. The error says how to get the value exactly instead.

**The marshaller is declared on the SIGNATURE, not applied at the call site**, and
that choice is the load-bearing one. The obvious alternative —
`record(scaled_of(price))` with `record` taking an `Int` — is weaker in exactly
the way §1 is about: the scale is gone from the type, so a `Decimal<4>`'s
unscaled integer type-checks identically, and so does an unrelated `Int`. **The
scale is lost at the boundary, which is the defect, not the fix.** Declared on
the signature, the scale IS the contract: `Decimal<4>` where `Decimal<2> as
scaled` was declared is a compile error, and every call site is then correct by
construction.

No `as text` marshaller was added: `c_fn(to_string(amount))` already does it
exactly (v0.0.28), and a feature whose only contribution is a second spelling
earns no place. The `CDouble` error points there by name.

**Linker pass-through, because an `extern fn` is only half an FFI.** Arguments
after the source file now go to the system linker unchanged
(`burxt run pay.bx cside.o -lm`), so the C being declared can actually be linked.
Burxt delegates linking to system tools and owns only object emission — the
position the platform roadmap already took. This is what let the guarantee be
tested against hand-written C rather than described: a test asserts `$19.99`
arrives as `1999`, that 2^53 crosses unchanged, and that 2^53+1 dies with a named
error instead of quietly becoming its neighbour.

Spec: `spec/N1-BOUNDARY-EXACTNESS.md`, with its own must-NOT list — no implicit
Decimal↔double conversion ever, no float type in Burxt, no "close enough" mode on
the range check, and no serialization layer yet (there is no encoder to guard;
when one is built it inherits these rules).

### v0.0.31: editor support — the half of a language that lives outside the compiler

A language is not real to the people using it until their editor knows it. This
version is that half, and it is deliberately the *declarative* half first.

**A TextMate grammar** (`editors/vscode/syntaxes/burxt.tmLanguage.json`) plus a
language configuration, packaged as a VS Code extension with **no JavaScript and
no build step** — it installs by being symlinked into place. The grammar knows
what makes Burxt Burxt, not just generic C-family shapes:

- `$19.99` and `8.25%` are numeric literals in their own right.
- `Decimal<2, RoundHalfEven>` highlights the scale and the rounding contract
  distinctly, because the contract is part of the type.
- `{interpolation}` inside a string is embedded code, `\{` is an escape, and a
  bare `}` is flagged **invalid** — the same thing the lexer does.
- `return tail f(...)`, `region name`, `dyn Trait`, and
  `amount: Decimal<2> as scaled` each read as what they are.

The same grammar is the artifact GitHub's Linguist consumes, so this is also step
one of `.bx` files being coloured on github.com.

**Verified, not assumed.** The grammar was run through the real TextMate engine
(`vscode-textmate` + Oniguruma) over a program exercising every construct, and
the token scopes were read back. A dependency-free test then locks the invariant
permanently: **every keyword and builtin the compiler knows must appear in the
grammar's patterns** — extracted from `src/lexer.rs` and `src/typeck.rs` at test
time rather than duplicated, because a duplicated list is the thing that drifts.
The test searches the grammar's *patterns* and not its prose, which was found by
mutation: the looser first version passed after the `tail` rule was deleted,
because the word survived in a comment.

**`burxt check`** — parse and typecheck only, no LLVM context and no linker. This
is what an editor or a CI gate calls, so it has to stay the cheapest way to ask
"is this program legal?".

**Two things this exposed, both fixed here:**

- **Nothing was checking the examples.** They are the first thing a newcomer
  reads, and they could rot silently while the suite stayed green. Every
  `examples/*.bx` now has to typecheck. Data files that other examples *read*
  moved to `examples/inputs/` — a directory rather than an exception list,
  because exception lists rot too.
- **The README described a version of Burxt that no longer existed** (no enums,
  no regions, no tail calls, no boundary exactness). Refreshed, since it is the
  front door.

**What is honestly NOT here:** diagnostics in the editor. Every compiler error
today is a precise sentence with **no position attached** — fine in a terminal,
useless to an editor, which needs a line, a column and a length to underline. So
source spans are the next piece of work, and an LSP after that. Building an LSP
first would be a shell with nothing inside it. A tree-sitter grammar (Neovim,
Helix) and a formatter are also recorded as not-built rather than implied;
`editors/README.md` holds the dependency order and the Linguist checklist,
including why `.bx` is not mislabelled as another language to fake detection.

### v0.0.32: errors that know where they are

```text
error: `*` on Decimal<2> needs an explicit rounding contract, because the exact
       result can have more than 2 decimal places. Declare one in the type, e.g.
       Decimal<2, RoundHalfEven> or Decimal<2, RoundHalfUp>.
 --> invoice.bx:3:1
  |
3 | let total: Decimal<2> = price * rate;
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Burxt's errors were always sentences a person could act on. What they lacked was
a **position** — fine in a terminal, useless to an editor, which needs a line, a
column and a length to underline. This is that missing half, and it is the
prerequisite the previous version named for everything editor-facing.

**Spans are byte ranges, and lines are a presentation concern.** The lexer knows
offsets for free; `LineIndex` converts to line/column once, at the edge. Storing
line/column everywhere instead would mean every layer agreeing on how to count a
tab. Columns count **characters**, so a `café` earlier on the line does not push
the caret one place right of what the reader sees.

**The interesting part is how little the error sites changed.** There are roughly
200 `Err(format!(...))` sites across the parser and typechecker, and not one of
them threads a span. Instead each stage attaches the position **once, at its
boundary**:

- The parser fails fast, so the token under the cursor when the error surfaces
  *is* the token the message is about.
- The typechecker records where it is on entering a statement or a top-level
  item, and attaches that on the way out. A nested statement naturally yields the
  **most precise** position, because it was the last thing entered.

That is why this landed as a refactor rather than a rewrite: the position was
recoverable from control flow that already existed.

**`--json` diagnostics.** `burxt check file.bx --json` emits one JSON object per
diagnostic, carrying 1-based line/column for humans *and* 0-based LSP positions,
because converting between them in the consumer is where off-by-ones live. Any
editor with a problem matcher can show squiggles today, without an LSP.

**A test that found five real bugs the moment it was written.** Every program in
`tests/fail/` is now required to report a position that points at *code* — not at
a comment or a blank line, which is the tell for a span that was never set.
Five of 226 failed: four validation paths (array returns, recursive structs,
incomplete impls, `dyn` returns) reported line 1 because they check *items*
rather than statements, and one pointed at the empty line after a file ending in
a newline. All five fixed — item passes now record the item's span, and an error
at end-of-file is reported on the last line with content, because "unexpected end
of file" pointing at a blank line is technically true and useless.

**A self-inflicted bug worth recording**, because the class recurs: adding
offset tracking meant routing every `self.chars.next()` through a `bump()`
helper — and the mechanical replacement rewrote the call *inside `bump` itself*,
so it called itself forever. Every program stack-overflowed instantly. The lesson
is the same one the codegen match-arm edits taught: **a global replace whose
pattern also matches the replacement's own body is a trap**, and the fix is to
check the helper after replacing, not to trust the sed.

**Deferred honestly:** expression-level spans. A type error underlines the whole
statement rather than the offending sub-expression, which is right about the line
and coarse within it. Also, a diagnostic inside a `{interpolation}` carries the
message but points at the string literal, because the interpolated fragment is
re-lexed on its own and its offsets are relative to the fragment. Both are
refinements of a working position, not missing positions.

### v0.0.33: a language server

```bash
burxt lsp      # diagnostics as you type, in any LSP-speaking editor
```

Positions existed as of v0.0.32, so the server has something to serve. It
typechecks the **buffer**, not the file on disk — which is the entire point of an
editor integration — and publishes one diagnostic or none.

**One diagnostic, honestly.** The compiler stops at the first error, so the server
does not pretend to a list. Reporting several is a *compiler* change (error
recovery), not a server change, and it is recorded that way so the limitation is
not mistaken for a server bug. **Publishing the empty list matters as much as
publishing an error**: it is what clears the squiggle when the code becomes valid,
and a server that only ever reports problems looks correct in a unit test while
leaving stale underlines in a real editor. The end-to-end test asserts exactly
that sequence — open valid (empty), break it (one error at the right line), fix it
(empty again).

**A JSON reader, written rather than depended on.** The compiler has exactly one
dependency (LLVM) and that restraint is worth keeping. The alternative people
reach for at this size — finding fields with string search — is wrong the moment a
document contains a quote or a backslash, which Burxt source does constantly. A
language server that mangles the buffer it was sent is worse than none. So
`src/json.rs` is a small, correct reader and writer, including surrogate pairs
(that is how an emoji in a document arrives) and integers that do not serialize as
`1.0` (some clients are strict). Its tests cover the malformed inputs too, because
a server that panics takes the editor's language support down with it.

**Details that are easy to get wrong and were tested instead of assumed:**

- `Content-Length` counts **bytes**, not characters — a message with a non-ASCII
  identifier would otherwise be truncated at the client.
- An unknown **request** must be answered (`-32601`), or a real client waits
  forever. An unknown **notification** must be ignored. The `id` field is the
  only difference.
- Full-document sync is requested deliberately: applying incremental text edits
  correctly is fiddly, and a server that corrupts its own copy of the buffer
  reports errors about code nobody wrote.

**Reaching editors.** Neovim (`editors/nvim/burxt.lua`, no plugin manager) and
Helix (`editors/helix/languages.toml`) attach the server directly; Zed, Emacs,
Sublime LSP and Kate need only the command. VS Code is the awkward one: launching
a server requires `vscode-languageclient`, which means npm and bundling — a real
cost against the extension's current property of being copyable with no
toolchain. Until that is paid, VS Code gets squiggles from a **problem matcher**
(`$burxt`) plus a task, which is declarative and needs no build step. The matcher
was verified against real compiler output rather than by reading the regex.

**Honest gaps, recorded rather than implied:** hover (the first thing worth adding
— `Decimal<2, RoundHalfEven>` on hover is worth more in Burxt than in most
languages), go-to-definition, and a tree-sitter grammar so Neovim and Helix get
colour and not only errors.

### v0.0.34: live diagnostics in VS Code, with no dependencies at all

Errors appear as you type, and the extension is still a directory you can copy
into place — no `npm install`, no `node_modules`, no bundler.

**How, given that an LSP client normally means npm.** It does not use the LSP. The
extension is plain CommonJS against the `vscode` API, which the editor injects at
runtime, and it runs `burxt check - --json`, feeding the buffer on **stdin**. Same
squiggles, no toolchain. `burxt lsp` remains the real server for every other
editor; switching VS Code to it buys hover and go-to-definition *when those exist*,
at the cost of a build step — worth paying then, not now.

**`burxt check -` reads the program from stdin**, which is the piece that made
this possible. What an editor has in its buffer is not what is on disk, and
checking the file would report errors the user already fixed. `run` and `build`
refuse `-`, because there would be no name for the executable.

**A wire format has consumers, so it is now tested as one.** The `--json`
diagnostic is read by the extension and will be read by CI gates. Renaming a field
would break them *silently* — the extension would simply stop showing squiggles,
with no error anywhere. So one test asserts the field names on **both sides at
once**: that the compiler emits them, and that `extension.js` reads the same ones.
It also asserts the positions stay 0-based, and that the extension invokes the
stdin form rather than checking the file on disk.

**Verified before shipping**, by driving the extension's exact pipeline from node:
spawn the compiler, feed a buffer, convert the JSON to a range, and print what
that range underlines. It underlines `let total: Decimal<2> = price * rate;` —
the offending statement, not the file. And a valid buffer yields zero diagnostics,
which is what clears the squiggle.

**Not fixed here, and not hidden:** one error at a time (a compiler change, error
recovery), and statement-level rather than expression-level underlining. Both
apply to every editor path, so they are recorded once rather than per client.

### v0.0.35: expression spans, sharper carets, and hover

```text
error: in the call to `tax`, argument 1 must be Decimal<2>, but it has type Int
 --> invoice.bx:3:11
  |
3 | print(tax(n) + $1.00);
  |           ^
```

Statement spans put the caret on the right line (v0.0.32). Expression spans put it
under the thing that is actually wrong — and they are what makes **hover** possible
at all, since answering "what is the type here?" means knowing which expression
*here* is.

**How the caret finds the smallest wrong thing.** `check_expr` became a thin
wrapper that, on failure, claims the position **unless something further in has
already claimed it**. A child's wrapper runs before its parent's as the error
propagates outward, so the innermost failing expression wins automatically — no
error site had to be touched. Where a parent's own check fails over children that
were each individually fine (a wrong argument, a value that disagrees with its
declared type), the parent says so explicitly with `blame(span)`, because there the
rule would be wrong: `let bad: Int = it.price;` should underline `it.price`, not
the whole line.

The bookkeeping lives in a `Cell`, not behind `&mut self`. Expression checking is
`&self`, and threading mutability through every checker method to carry a
diagnostic detail would claim it was part of the checking. It is not.

**Hover, and why it is worth more in Burxt than elsewhere.**

```text
Decimal<2, RoundHalfEven>

Exact decimal, 2 decimal places. A result that needs rounding rounds half to even
(banker's rounding).
```

The type names the scale; the sentence names what happens when a result does not
fit that scale, which is the whole question this language exists to make visible.
`CDouble` says a Decimal may not cross as one. A bare `Decimal<2>` says any
operation that could round is a compile error until a contract is declared.

The checker now records `(span, type)` for every expression it gets through, and
hover picks the **smallest** span containing the cursor — because expressions nest,
and the cursor on `qty` in `price * qty` should say `Int`, not the product's type.

**Two honest limits, both tested rather than footnoted:**

- Hover knows types **up to the first error and nothing past it**, because the
  compiler stops there. So hover goes quiet below a mistake and returns when it is
  fixed. That is error recovery's job, not the server's.
- The `let`-mismatch caret moved from the whole statement to the value, which
  broke two tests that had recorded the old, coarser positions. Both were updated
  to the sharper expectation — worth noting because a test that encodes a position
  is exactly the test that should fail when positions improve.

**And one test caught its own premise being wrong:** the end-to-end session used
`textDocument/hover` as its "unsupported method" probe. Hover is supported now, so
the probe moved to `textDocument/definition` and the test gained an assertion that
hover actually answers with a type.

### v0.0.36: VS Code speaks to the language server

Hover shipped for every LSP-speaking editor in v0.0.35 — except VS Code, which was
on a private `burxt check --json` path. Now it uses the same server as everyone
else, and still needs **no `npm install`**.

**A hand-written LSP client, about a hundred lines**, instead of
`vscode-languageclient`. That package would bring npm, a lock file and a bundling
step, and the property worth protecting is that `editors/vscode/` is a directory
you copy into place and use. What the client has to get right is small and
well-defined: frame messages out, unframe them in, match responses to requests by
id, and pass notifications along.

**The one detail that decides whether it works: buffer BYTES, not a string.**
`Content-Length` counts bytes, so accumulating stdout as a string and slicing on
that count corrupts every message containing a non-ASCII character — and Burxt
programs contain `café` and `€` in string literals routinely. The test asserts
`Buffer.concat` is used, with the reason written next to it.

**Why using the server matters more than the line count:** there is now exactly one
place where "what does the compiler know about this buffer" is answered, and every
editor asks it the same way. The `--json` path stays supported for tasks and CI —
`.vscode/tasks.json` and the `$burxt` problem matcher both use it — but it is no
longer a second implementation of the editor experience.

**The client is tested rather than inspected.** VS Code cannot be scripted here;
the client can. `editors/vscode/test/harness.js` stubs the `vscode` module, drives
the real `extension.js` against a real `burxt lsp`, and checks the whole loop:
a valid buffer publishes an empty list, a broken one publishes exactly one
diagnostic positioned at the offending value, fixing it clears the squiggle, hover
returns `Decimal<2, RoundHalfEven>` with its contract explained, and hover on
whitespace returns null rather than a guess. `cargo test` runs it when node is
available and **says loudly when it skips** — the Rust suite must not require a
JavaScript toolchain, but a check this valuable should not quietly not run.

These are exactly the failures that look fine on inspection: a message split across
chunks, a byte length applied to a string, a promise that never resolves.

### v0.0.37: every mistake at once

```text
error: type mismatch in `let wrong`: declared Bool, but expression has type Int
 --> many.bx:3:19
  |
3 | let wrong: Bool = qty;
  |                   ^^^

error: type mismatch in `let bad`: declared String, but expression has type Decimal<2>
 --> many.bx:5:19
  ...

3 errors
```

The typechecker no longer stops at the first problem. It records it, recovers, and
carries on — so a file with three mistakes reports three, in source order, instead
of making the reader fix one, recompile, and discover the next five times over.

**Burxt turns out to be unusually good at this, for a reason worth recording:
every `let` declares its type.** The hard part of error recovery elsewhere is that
a failed initializer leaves a binding with no type, so every later use of it
produces a second, invented error — the cascade that makes recovery worse than
useless. Here the annotation was mandatory all along, so a statement that fails
still contributes a **correctly typed name**, and the rest of the function checks
against the type the author asked for. The test asserts both halves: all three
real errors, and *nothing else* — no "unknown name" noise from the two later
statements that use the failed bindings.

**Two things deliberately still report alone:**

- **Lexer and parser errors.** Recovering a token stream means guessing where a
  malformed statement ends, and a wrong guess *invents* errors rather than finding
  them. Asserted by its own test so the distinction stays a decision.
- **Declaration errors** — a bad struct field, an unknown type in a signature.
  Continuing past those means checking a function whose types are unknown, which
  produces confident nonsense.

**Two follow-on effects, one of which reversed an earlier test:**

- **Hover now works below a mistake**, not just above it. The v0.0.35 test asserted
  the opposite ("hover goes quiet below a mistake, and comes back when it is
  fixed") and was correct at the time; recovery is what changed it. The test now
  asserts hover answers on *both* sides of an error.
- **The return-path proof had to become conditional.** A body with a failed
  statement produces no `TypedStmt` for it, so "must end by returning" would fire
  as a second complaint about the same mistake. It now runs only when the body
  checked cleanly.

The language server publishes all of them, so an editor underlines every place at
once; `--json` emits one object per line, already in source order, each error only
once.

### v0.0.38: functions that allocate in the caller's region

```text
fn describe(line: Int, byte: Int) -> String allocates {
    return "line " + to_string(line) + ": unexpected byte " + to_string(byte);
}

region source { print(describe(3, 108)); }
```

**A helper could not build a String and return it, and that blocked the
self-hosted compiler more than anything else.** Every error message, every rendered
type name, every `Int`-to-text conversion in a library function needs exactly this
shape — and both routes were closed. A plain function body has no region, so the
allocation was refused; opening one inside the function meant the result could not
be returned, because that region ends at the closing brace.

**The fix rests on something that was already true.** A function called from inside
a region *already* allocates in that region — the allocator is a bump pointer, and
the mark belongs to the caller. So the value never outlives its region and M1's rule
was satisfied all along. The compiler simply had no way to know a function intended
this, and refused conservatively.

`allocates` on the signature says it: **this function builds values in its caller's
region.** It may allocate without opening one, it may return what it built, and
every call site must have a region open.

**Declared, not inferred, and the reason matters.** Inference is entirely possible —
walk the call graph, propagate. It was rejected because it would be the only
invisible contract in the language. Every other guarantee Burxt makes is written
where it applies: a rounding contract in the type, `dyn` at the dispatch site,
`tail` at the call, `as scaled` at the boundary. A function that quietly acquires a
requirement on all its callers because someone added a `+` deep inside it is the
action-at-a-distance the rest of the language refuses. Being declared also makes it
decidable in one pass, since signatures are hoisted before any body is checked.

It is **not a lifetime** — no name, no scope relation, nothing to unify. One bit,
which is why it can be a keyword rather than a parameter, and why M1's "no lifetimes
in signatures" still holds.

**What still fails, and must:** a value built inside a `region` block the function
itself opened cannot be returned (that region really does end); and a caller cannot
return an `allocates` call's result out of its own region, because such a call now
*counts* as allocating at the call site, so the caller's escape rules govern it
exactly as if it had built the value itself.

**Codegen did not change at all.** If it had needed to, the reasoning above would
have been wrong.

**The payoff, in the self-hosted lexer:** `examples/lexer.bx` now reports
`byte 64 at offset 177 starts no token` — a message *built* by Burxt code rather
than printed piecemeal. The requirement is visible up the whole chain: `unexpected`,
`show` and `tokenize` each say `allocates`, because each calls something that does.

**Two things this shook out:**

- **Every "needs a region" message is now written once**, in one helper, and offers
  both fixes. They had drifted into four slightly different sentences.
- **A `match` arm's pattern error pointed at the previous arm.** Checking the
  arm above had moved the recorded position. Found the honest way: by shadowing a
  name in `examples/lexer.bx` and being sent to the wrong line.

Spec: `spec/M1a-CALLER-REGION-FUNCTIONS.md`, with its own must-NOT list — no
inference, no region names in signatures, no implicit region at a call site, no
`allocates` on `extern fn`, and no codegen change.

### v0.0.39: `pure` — reproducibility the compiler checks (NOVELTY §2, slice 1)

```text
pure fn interest(balance: Decimal<2, RoundHalfEven>, rate: Decimal<4>)
    -> Decimal<2, RoundHalfEven>
{
    return balance * rate;
}
```

> **This function's result depends only on its arguments. The compiler checked.**

Auditors and regulators care intensely whether a calculation is reproducible, and
today the honest answer in every language is *"we believe so."* A hidden
`DateTime.Now`, a locale-dependent parse, or a config lookup three calls down
silently makes a computation irreproducible, and nothing catches it. Burxt already
guarantees the arithmetic is exact and byte-identical across targets; `pure` extends
that to the **inputs** — nothing may enter the calculation except through a
parameter.

**The register listed this as needing an effect system first. It needed less than
that**, because v0.0.38 introduced the first declared effect marker (`allocates`).
`pure` is the same shape pointed the other way: a marker that **forbids** rather than
permits. A `pure fn` may not print, may not read a file, may not call into C, and may
not call a function that is not itself `pure` — which makes the property transitive
without inferring anything.

**What it may do, deliberately: allocate.** A bump allocator observes nothing about
the outside world and returns the same layout for the same sequence of calls, so
`pure fn render(...) -> String allocates` is legal and useful — a pure function that
builds a string. The two markers compose because they describe different things: one
says *where memory comes from*, the other says *what may influence the result*.

**Purity constrains the callee, never the caller.** Any function may call a pure one,
nothing propagates upward, and the marker can be adopted one function at a time.

**Honest about today's teeth, because overselling this would be worse than not
shipping it.** Burxt has no clock, no random, no locale, no environment access and no
ambient configuration. So the rules bite on **I/O and the FFI** — which is where
nondeterminism actually enters a Burxt program today — and are otherwise a **forward
guarantee**: when a clock is added it will be added *behind* this rule rather than in
front of it.

**Deliberately not done:** no inference (`pure` is written where it applies, like
every other guarantee in the language), no opt-out inside a pure function, and **no
purity-driven optimisation**. Memoisation and common-subexpression elimination are
things this guarantee enables, and doing them now would mean the marker changes
behaviour as well as legality. In the version that introduces it, it must only ever
change what compiles.

Methods cannot carry the marker yet, so a pure function cannot call one — refused
with that reason stated, and `pure fn (self: T) ...` is refused at the parser with
the same explanation rather than a confusing message about tokens.

Spec: `spec/N2-PURE-FUNCTIONS.md`.

### v0.0.40: the brand, in place

Andre's artwork, organised and wired in. The mark is `><` — two chevrons converging
— and the wordmark is `Burxt` with that mark **as** its `x`. Reading the name and
seeing the logo are one act, and what the mark means is *exact*: two things meeting
at a position that is fixed rather than approximate.

- `assets/` holds the kit: the icon at favicon sizes, transparent and on an obsidian
  tile, a multi-size `.ico`, the wordmark, and lockups on transparent, light and
  dark grounds.
- The VS Code extension uses the **tile** for its marketplace icon (the extensions
  list shows it on its own background, so a filled tile is right) and the
  transparent 48px icon for `.bx` files in the explorer. Copper reads on both light
  and dark themes, so one file serves both.
- The README banner switches on `prefers-color-scheme`.

**The palette was sampled from the artwork, not eyeballed**: copper `#b26436`,
obsidian `#232320`, read pixel by pixel out of `burxt-favicon-512.png`. Anything
that needs the brand colour as *text* — the GitHub Linguist entry, a future
stylesheet — now uses the value the artwork actually contains, instead of the
placeholder green I had guessed earlier.

The extension keeps its own copies of two files, because VS Code resolves
contributed paths relative to the extension directory rather than the repository;
`assets/README.md` records the two `cp` commands to re-run if the artwork changes,
next to Andre's original notes rather than edited into them.

### v0.0.41: the mark on `.bx` files

The extension declared an icon for the `burxt` language in v0.0.40 and nothing
appeared, which is worth recording because it looks like a bug and is not.

**VS Code has no supported way to add one icon on top of another icon theme.** A
file icon theme is monolithic. The default **Seti** theme ignores
language-contributed icons entirely, and the built-in **Minimal** theme does too —
its `languageIds` map is literally empty, which I checked in the shipped theme file
rather than assuming. So the declaration alone can never show anything.

So the extension now ships a **file icon theme**: the copper mark for `.bx`, and a
plain document, folder and open-folder for everything else. It sets
`showLanguageModeIcons: true`, so any language that contributes its own icon gets
it — and the reason a default document glyph is needed at all is that **zero
built-in languages contribute one** (also measured, by scanning every built-in
extension's manifest). A `showLanguageModeIcons` theme with no fallback would leave
every other file blank.

Deliberately minimal, and it says so in the docs rather than pretending: this is not
an attempt at a four-hundred-glyph icon set, it is three utility shapes and the
brand mark. `editors/README.md` records the two alternatives that keep rich icons
for everything else — `vscode-icons`' custom icon folder, or any theme that already
opts into language icons.

This repository turns the theme on in `.vscode/settings.json`, which is
workspace-scoped: it applies here, nowhere else, and one deleted line reverts it.

### v0.0.42: a real extension, and a correction

**v0.0.41 was wrong, and the way it was wrong is worth more than the fix.** I claimed
the default Seti theme ignores language-contributed icons, and shipped a whole file
icon theme to work around it — at the cost of every other file's icon. That claim was
an assumption. VS Code's own logic, read out of the shipped bundle:

```js
n = true                     // set when a theme defines languageIds
showLanguageModeIcons === true || (n && showLanguageModeIcons !== false)
```

Seti defines 83 `languageIds` and never sets the flag to `false`, so language icons
**do** apply to any language Seti does not itself cover. `contributes.languages[].icon`
was correct all along — the same mechanism `apex-stack.apex-alpine` uses, which is
what Andre pointed at. The icon theme is removed, along with the workspace setting
that turned it on. No theme to switch, nothing lost.

The lesson is the one this project already applies to the compiler: **check the
mechanism instead of reasoning about it from memory.** The answer was in a file on
disk the whole time, and finding it took one grep.

**What was actually missing was installation.** The extension had been *symlinked*
into the extensions directory, which works until something reads the extension
registry and does not find you. So it is now packaged and installed properly:

```sh
python3 editors/vscode/pack.py                            # no npm, no vsce
code --install-extension editors/vscode/burxt-0.1.0.vsix
```

`pack.py` is a .vsix writer in the standard library — a .vsix is a ZIP holding an OPC
content-types map, a VSIX manifest and the extension under `extension/`. `vsce` does
more (linting, dependency bundling, marketplace checks), all of it for *publishing*
rather than installing, so none of it is needed. The extension keeps its promise of
needing no toolchain.

**One manifest property that matters on a remote:** `"extensionKind": ["workspace"]`.
Without it, a WSL or SSH session runs the extension on the **UI** side, where there
is no compiler and no language server to talk to. Now asserted by a test, along with
the language icon declaration and the existence of every file `pack.py` ships —
three things whose loss is silent.

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

Spec: `spec/A5-CONTRACTS.md`.

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

Spec: `spec/N5-TERMINATION.md`.

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

### v0.0.51: the primitives that make a program a tool

Phase 1 of `spec/M4-SELF-HOSTING.md`, which is now the plan of record with measured
numbers in it rather than an intention.

**`arg_count()` and `arg(n)`.** A compiler has to know which file it was asked to
compile, and the C runtime only offers that to `main` — so `main` now takes `argc` and
`argv` and stashes them where any function can read them. `arg(n)` is bounds-checked
like everything else, and needs **no region**: the runtime's argument strings outlive
the program, so it borrows rather than copies. That is the first String-producing
builtin that does not allocate, and the reason is worth stating rather than looking
like an oversight.

**`write_file(path, contents)`** returns the number of bytes written, so a caller can
check rather than hope. Refused inside a `pure` function, for the reason every effect
is: a function whose result depends only on its arguments does not leave marks.

**A region a compiler can live in.** The bump allocator's chunk went from 64 MB to
1 GB. Stage-1 holds an arena of AST nodes, a symbol table and every interned name for
one whole compile inside a single region, and 64 MB would not have survived it. The
cost is **virtual, not resident** — `malloc` of that size hands back lazily mapped
pages, so a program that touches a kilobyte pays for a kilobyte. Exhaustion is still a
named error rather than an overrun.

**And the plan itself is now in the repository**, with the sizes measured from the
Rust compiler (11.5k lines; stage-1 needs ~10–12.5k of Burxt), the phases, the public
milestone at the end of phase 4, and the risks named — including the one that quietly
kills bootstraps, which v0.0.50 verified is absent: three compiles of the same file
produce byte-identical IR, and no HashMap is iterated to produce output.

The spec also records the decision that makes the backend feasible at all: **stage-1
emits textual LLVM IR.** It cannot drive LLVM's C API, because `extern fn` returns are
`Int`/`CInt` only — Burxt refuses to receive a pointer whose ownership it cannot
describe, so an `LLVMBuilderRef` is unreachable *by construction*. Emitting text is
simpler anyway: string formatting instead of a builder, and output you can diff.

### v0.0.52: the stage-1 lexer, and it lexes itself

M4 phase 2. `examples/stage1_lexer.bx` is 376 lines of Burxt and is **not** a
demonstration: every punctuation form including the eight two-character ones, a
39-entry keyword table with type names distinguished from identifiers, string literals
with escapes and interpolation detection, comments, and exact money and percent
literals.

```text
lexed examples/tour.bx: 393 tokens, 39 keywords known
  decimal $19.99 -> unscaled 1999 scale 2
```

`$19.99` becomes the unscaled integer **1999 with scale 2**, accumulated digit by
digit — the thesis holding inside the self-hosted lexer, not just in the compiler that
compiled it. A percent literal comes out two places finer, exactly as the Rust lexer
makes it.

**It lexes its own source**: 3,131 tokens, zero errors. And a new test makes that a
standing guarantee rather than an anecdote — the Burxt lexer is run over **every
Burxt source in the repository**, including itself and all 81 programs in the pass
suite. Those files already compile, so the Rust lexer accepts them by definition;
any byte the Burxt lexer refuses is a **disagreement between two implementations**,
and one of them would be wrong. That is the first cross-check between stage-0 and
stage-1, and the shape every later phase will reuse.

**What the language made awkward, honestly.** Token kinds are `Int` codes rather than
an enum, because a 60-variant enum would force a 60-arm `match` at every use and the
payloads differ per kind. That is a real cost of exhaustive matching without a
wildcard — the rule that has caught genuine bugs elsewhere is a nuisance here. Kept,
because the alternative is `_`, which v0.0.20 refused on purpose.

**And a small thing the compiler got right unprompted:** three scanners had to be
declared `allocates`, because building `"error: byte " + to_string(one)` allocates —
and the compiler said so, naming the fix, in a file it had never seen before.

### v0.0.53: the stage-1 parser — types, expressions, statements

M4 phase 3a. `examples/stage1_lexer.bx` became `examples/stage1.bx`, because it is no
longer a lexer: it is the stage-1 compiler, growing a phase at a time in one file, which
is the shape the plan predicted while Burxt has no modules.

**1,009 lines of Burxt** now — 376 of lexer and 633 of parser: every type form (including `Decimal<S, R>`, slices, fixed
arrays and `dyn`), the full expression precedence ladder with postfix chains, struct and
array literals, and every statement — `let`, assignment, `print`, `return`, `return
tail`, `if`/`else if`, `while`, `break`, `continue`, `region`, `match` with payload
bindings, and expression statements.

**It parses every Burxt source in the repository, including its own**, and the
cross-check test now covers both halves: any construct the Burxt parser refuses is a
disagreement with the Rust parser, and one of them is wrong.

**The arena design changed, and for a better reason than the one that forced it.**
Child lists — a call's arguments, a block's statements, a match's arms — live in a
side array of indices, with a node holding `(start, count)`. That began as a workaround:
Burxt has no `xs[i].field = v`, and cannot write to a growable array element through a
field, so the obvious linked-cell approach could not back-patch. But children pushed
into a side array land **contiguously** even though their subtrees interleave in the
node array, so a list is two integers instead of a chain — which is what production
compilers do anyway. **The language's limitation pushed the design somewhere better.**

**Three gaps found, each recorded with its trigger:**

- **`xs[i].field = v`** — assignment through an index and then a field. Utterly
  ordinary (`table.rows[i].count = 5`), so it earns its place; deferred to keep this
  phase shippable.
- **Writing to a growable array element through a field** — `self.nodes[i] = value`.
  Reading works, `push` works, writing does not.
- **The highlighter disagreed with the compiler about `\}`.** The compiler accepts it
  as an escape; the TextMate grammar's escape list did not include it, so valid code
  was flagged invalid. Fixed. That is the second time writing Burxt found a drift the
  keyword test could not see, because it checks that keywords *exist*, not that escape
  rules match.

**And a bug in my own Burxt code worth keeping.** The driver steps over items it does
not parse yet by matching braces, and treated any semicolon before the first brace as
the end of a bodyless `extern` declaration. `fn f(xs: [Int; 3])` contains a semicolon —
inside the array type — so the skip stopped mid-signature. Fixed by counting
parentheses and brackets too. A heuristic that had to meet real syntax to fail.

Items — `fn`, `struct`, `enum`, `trait`, `impl`, `extern` — are phase 3b. The driver
steps over them rather than pretending to read them.

### v0.0.54: stage-1 parses items — and parses itself

M4 phase 3b. `fn`, `pure fn`, methods with `mut self` receivers, `struct`, `enum` with
payloads, `trait` signatures, `impl Trait for Type`, `extern fn` — with the markers and
contract clauses that make a Burxt signature say what it promises: `allocates`,
`requires`, `ensures`, `decreases`, and `as scaled` on a parameter.

**The number that matters:**

```text
parsed 55 items and statements into 6610 nodes, 2263 child slots
  parse errors: 0            <- stage1.bx, parsing its own 1,300 lines
```

Every function, every method, every struct, every contract clause in the stage-1
compiler, read by the stage-1 compiler. The front end is now **self-parsing**, and the
cross-check test holds it to that over every source in the repository.

**The language caught me using its own keyword.** `let mut allocates: Int = 0;` — refused,
because `allocates` became a keyword in v0.0.38 and Burxt does not let a name shadow one.
The variable is now `builds_in_caller`, and the refusal was correct: a local called
`allocates` inside the parser that *handles* `allocates` is exactly the confusion the rule
exists to prevent.

**Markers ride as bits, and that is a deliberate arena decision.** `pure`, `allocates` and
a mutating receiver are three flags in one integer field rather than three fields, because
a node is a fixed-size struct in an array and every field is paid for by *every* node.
The same reasoning that makes real compilers pack their AST.

**What is left of the front end:** interpolation fragments are detected but not split into
pieces, and the parser records enough to rebuild a signature but not the receiver's
parameter list on a trait signature. Both are named in the spec rather than left to be
discovered.

Next is phase 4, the typechecker, which the plan calls the hardest and the largest — and
which is where the public milestone sits.

### v0.0.55: the marker words become contextual

```text
let mut allocates: Int = 0;          // legal now — an ordinary name
fn label(n: Int) -> String allocates // still the marker, in the one place it means one
```

**Prompted by a question from Andre**, after v0.0.54 hit the collision: PHP's `$var`
makes reserved-word conflicts impossible, so why doesn't everyone do that?

The answer is where the cost lands. A sigil taxes **every variable reference** —
millions across a codebase — to refund a problem that happens a dozen times in a
language's life, and it does not even remove the reserved list (PHP still forbids
`class class`, and `$this` is reserved). Perl's sigils at least encode *type*;
PHP inherited them and they encode nothing. The interpolation benefit that makes
sigils worth it in shell, Burxt already gets from a delimiter — `"total {amount}"`
with `\{` for a literal — which costs something only inside strings.

The languages that took the problem seriously solved it precisely: **contextual
keywords** (C#'s `async`, `await`, `yield`, `value` are all legal identifiers) and
**raw identifiers** (Rust `r#type`, Swift backticks). Both pay only at the collision.

**And Burxt has the problem worse than most**, because its philosophy makes it worse:
every guarantee is a declared word, so the list grows with every feature — `pure`,
`allocates`, `tail`, `requires`, `ensures`, `decreases`, and more to come.

So `allocates`, `requires`, `ensures` and `decreases` left the keyword table. Each
appears in exactly **one** position — after a return type, or between a signature and
a body — where nothing else can appear, so the parser recognises them by place rather
than by reservation. Everywhere else they are names.

**There was already a precedent in the language:** `scaled` in `as scaled` was
contextual from the day it shipped (v0.0.30) and never reserved. This makes the rest
consistent with it.

**Strictly loosening, which is why it is safe.** Programs that were errors become
legal; no valid program changes meaning. That is what the v0.0.17 syntax-change law
requires, and it is the opposite direction from the change that law was written for.

**What stays reserved, and why:** `pure`, `tail`, `let`, `if`, `break` and the rest can
begin a statement or an expression, where an identifier can also begin one. Recognising
those by position would be genuine ambiguity rather than free precision. The line is not
"which words are keywords" but "which words have exactly one possible position".

### v0.0.56: stage-1 follows stage-0, and the cross-check proved its worth

A correction, and the best evidence yet for building the second implementation early.

v0.0.55 made four marker words contextual in **stage-0** and shipped with a failing
test — I chained the commands and committed before reading the result, which is the
same mistake as the `grep -c` one in v0.0.44: **not looking at the answer.**

What failed was exactly the right thing. The front-end cross-check compiles the Burxt
lexer and parser and runs them over every source in the repository, and it reported:

```text
tests/pass/contextual_markers.bx: the Burxt PARSER reported an error the Rust parser
did not
```

Stage-1's own keyword table still held `allocates`, `requires`, `ensures` and
`decreases`, so `let mut allocates: Int = 0;` — a program stage-0 had just started
accepting — was a syntax error to stage-1. **Two implementations of the same language
disagreeing, caught within a minute, by a test written two versions earlier for
exactly this.**

Stage-1 now recognises the four by position too, comparing the token's span against
the word without allocating — the same trick its keyword lookup uses.

**The lesson is about method, not about markers.** A second implementation is not only
the M4 certificate; it is a differential test. A change to the language now has two
places that must agree, and the disagreement surfaces as a failing test rather than as
a bug report six months later. That is worth more than the milestone.

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
  dispatch (v0.0.14) — **DONE and CLOSED.** `class` / `open` single inheritance
  was dropped in v0.0.46: thirty versions of real programs never needed it.
- A4.7. Signature grammar: money/unit literals (`$19.99`, `8.25%`), string
  interpolation as a print (v0.0.17) and as a value (v0.0.28) — DONE. Unit
  literals (`5.km`) and pipelines still to come.
- A4.8. File input: `read_file` and `to_string` (v0.0.28) — the two things a
  self-hosted compiler could not do without.
- A4.9. Guaranteed tail calls: `return tail f(...)` lowered to `musttail`
  (v0.0.29) — NOVELTY §4, the first novelty-register entry to ship.
- A4.7b. Contextual marker words (v0.0.55): `allocates`, `requires`, `ensures` and
  `decreases` are recognised by position, not reserved — so they are usable as names.
- M4 phase 3b (v0.0.54): items, markers and contract clauses — stage-1 parses its own
  source into 6,610 nodes with no errors.
- M4 phase 3a (v0.0.53): the stage-1 parser — all types, the full expression ladder,
  every statement form, in an arena with contiguous child lists. Parses its own source.
- M4 phase 2 (v0.0.52): the stage-1 lexer in Burxt — the real token set, exact money
  literals, and a cross-check that it accepts every Burxt source in the repository
  including its own.
- M4 phase 1 (v0.0.51): `arg`, `arg_count`, `write_file`, and a 1 GB lazily-mapped
  region — the primitives a self-hosted compiler cannot start without. Plan of record:
  `spec/M4-SELF-HOSTING.md`.
- A5.0b. `break` and `continue` (v0.0.50), with the region-release rule a jump out of
  a loop needs. Deferred since v0.0.11 until three self-hosted programs worked around
  their absence.
- M4b. Self-hosting, fourth piece (v0.0.49): the scale rule in Burxt — matching scales
  for `+`, a mandatory rounding contract for `Decimal * Decimal`, and the product's
  exact scale computed. The thesis checking itself.
- FIX (v0.0.48): `expr_allocates` now sees through struct literals, enum payloads and
  array literals. Region data could previously escape inside an aggregate — a
  use-after-free the checker accepted, found by designing a self-hosted checker's error
  type.
- M4a. Self-hosting, third piece (v0.0.47): `substring`, `allocates` on methods, and
  a symbol table written in Burxt that catches a redeclaration in a real `.bx` file.
- A2a. Integer division by name (v0.0.46): `div_floor`, `div_trunc`, `rem`, each
  checked for zero and for the one quotient an i64 cannot hold.
- A4.6 CLOSED (v0.0.46): `class` / `open` inheritance dropped; composition-only final.
- N5. Termination measures (v0.0.45): `decreases`, checked at every recursive call
  site — which is what makes it work with guaranteed tail calls. NOVELTY §5.
- A5a. `old(...)` and method contracts (v0.0.44): NOVELTY §3's conservation laws,
  checked at runtime with the law quoted on failure.
- A5. Contracts, slice 1 (v0.0.43): `requires` / `ensures` checked at runtime, the
  clause quoted when it fails, and required to be pure. NOVELTY §3's staging.
- N2. `pure` functions, slice 1 (v0.0.39): reproducibility checked at the signature
  — no I/O, no FFI, no impure calls. NOVELTY §2.
- M1a. Caller-region functions (v0.0.38): `allocates` on a signature, which
  unblocks returning built values — the biggest remaining obstacle to a
  Burxt-hosted compiler.
- T7. Error recovery (v0.0.37): every type error at once, cascade-free because
  `let` always declares its type. Parse and declaration errors still report alone,
  on purpose.
- T6. VS Code on the language server (v0.0.36): a dependency-free LSP client,
  hover in VS Code, and a node harness that drives the extension against a real
  server.
- T5. Expression spans, sharper carets and hover (v0.0.35): `blame` for
  parent-owned errors, a `(span, type)` table, and hover that explains rounding
  contracts.
- T4. Live diagnostics in VS Code (v0.0.34): `burxt check -` from stdin, a
  dependency-free extension, and a test locking the JSON wire format on both
  sides.
- T3. Language server (v0.0.33): `burxt lsp` over stdio, a hand-written JSON
  reader/writer, editor configs for Neovim and Helix, and a VS Code problem
  matcher. Hover and go-to-definition still to come.
- T2. Diagnostics with positions (v0.0.32): spans through lexer/parser/typeck,
  a caret rendering, `--json` output with LSP positions, and a suite-wide test
  that every rejection points at real code.
- T1. Editor support (v0.0.31): TextMate grammar, VS Code extension,
  `burxt check`, and a test locking the grammar to the compiler's keyword table.
  Diagnostics and an LSP wait on source spans.
- N1. Boundary exactness, slice 1 (v0.0.30): `CDouble` as a nameable lossy
  foreign type, `Decimal as scaled` marshallers declared on the signature,
  range-checked `Int` → `CDouble`, and linker pass-through so the C being
  declared can be linked. NOVELTY §1.
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

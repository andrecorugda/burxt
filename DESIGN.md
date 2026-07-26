# Burxt — Design Notes (v0.0.32)

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
- A4.7. Signature grammar: money/unit literals (`$19.99`, `8.25%`), string
  interpolation as a print (v0.0.17) and as a value (v0.0.28) — DONE. Unit
  literals (`5.km`) and pipelines still to come.
- A4.8. File input: `read_file` and `to_string` (v0.0.28) — the two things a
  self-hosted compiler could not do without.
- A4.9. Guaranteed tail calls: `return tail f(...)` lowered to `musttail`
  (v0.0.29) — NOVELTY §4, the first novelty-register entry to ship.
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

# Burxt — Design Notes (v0.0.109)

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
future type (record field-wise equality, string byte equality, Option)
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
  opt-in and visible (`let mutable`, v0.0.4).
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
  currencies to be distinct types (nominal records already give this shape);
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
> The reason is evidence, not taste. Traits + `implement` + composition shipped in
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
    function describe(self) -> String
}

// Composition is the natural default: Account HAS-A Ledger, not IS-A.
class Account : Printable {
    owner:   String
    balance: Decimal<2> = $0.00
    ledger:  Ledger                 // a field, not a parent

    function describe(self) -> String {
        "Account of {self.owner}: {self.balance}"
    }
}

open class Shape { function area(self) -> Decimal<4> }
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
| Dependency Inversion | Depending on a trait is ergonomic (`dynamic Trait` as a parameter); depending on a concrete type is the awkward opt-in. |

## Signature grammar — eloquent because it matches intent (committed)

95% familiar, 5% novel exactly where the thesis lives. Eloquence comes from
grammar matching the domain so closely that correct code reads like a
description of the problem.

### Contracts as first-class grammar

**This supersedes contracts-as-attributes below.** `requires` / `ensures` are
KEYWORDS, so a function reads as a self-documenting sentence:

```text
function withdraw(acct: Account, amount: Decimal<2>)
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
- Stage 1: **DONE (v0.0.73)** — the Burxt compiler rewritten in Burxt,
  compiled by stage 0, and then compiled by *itself*: stage-2's IR is
  byte-identical to stage-1's, which is the fixpoint. "Burxt compiles Burxt" is
  true, with the scope stated honestly in `spec/M4-SELF-HOSTING.md` §3b — the
  Burxt backend does not emit every construct yet, and stage 0 stays as the
  trust anchor and the differential test.

## Milestone log

The log is in **[`docs/log/`](docs/log/)**, one file per stretch of versions, indexed in
[`docs/log/README.md`](docs/log/README.md). It was in this file until v0.0.72, when 2,500 of
its 3,000 lines were log: the design a reader comes here for was buried under the history of
how it got here, and finding an entry meant searching rather than navigating.

| Versions | What happened | |
|---|---|---|
| **v0.0.1–v0.0.10** | The language runs | [read](docs/log/01-the-language-runs.md) |
| **v0.0.11–v0.0.20** | Aggregates, dispatch, and the literals money needs | [read](docs/log/02-aggregates-and-dispatch.md) |
| **v0.0.21–v0.0.30** | Memory, regions, and the first self-hosted pieces | [read](docs/log/03-memory-and-the-first-self-hosting.md) |
| **v0.0.31–v0.0.37** | The half of a language that lives outside the compiler | [read](docs/log/04-tooling.md) |
| **v0.0.38–v0.0.42** | `allocates`, `pure`, and the mark | [read](docs/log/05-allocates-pure-and-the-brand.md) |
| **v0.0.43–v0.0.50** | Contracts, conservation laws, and termination | [read](docs/log/06-contracts-and-termination.md) |
| **v0.0.51–v0.0.58** | The front end, in Burxt | [read](docs/log/07-the-self-hosted-front-end.md) |
| **v0.0.69–v0.0.99** | The mark, the fixpoint, the compiler's own speed, the ergonomics, generics with bounds, no null, `?`, every keyword spelled the word it means, and the tooling held to the same standard | [read](docs/log/08-the-mark-and-the-tree.md) |

v0.0.59–v0.0.68 and v0.0.70 have no log entry: they were ten consecutive versions of one
milestone, recorded in [`spec/M4-SELF-HOSTING.md`](spec/M4-SELF-HOSTING.md) next to the plan
their measurements were checked against. The index says so plainly rather than leaving a
reader to notice the numbers skip.

**How entries are written**, unchanged by the move: appended, never rewritten. A superseded
decision is marked superseded and the original reasoning is kept, because the reasoning is
usually still right and only the conclusion moved. A version spent on a mistake says what
the mistake was — that is the part worth reading later.

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
- A4. Strings (v0.0.7), records (v0.0.8), arrays (v0.0.10) — DONE
- A4.5. The aggregate ABI: `byval` params, `sret` returns, layout guarantee
  — DONE (v0.0.12)
- A4.6. Composition-first OOP: receiver methods (v0.0.13), traits + `dynamic`
  dispatch (v0.0.14) — **DONE and CLOSED.** `class` / `open` single inheritance
  was dropped in v0.0.46: thirty versions of real programs never needed it.
- A4.7. Signature grammar: money/unit literals (`$19.99`, `8.25%`), string
  interpolation as a print (v0.0.17) and as a value (v0.0.28) — DONE. Unit
  literals (`5.km`) and pipelines still to come.
- A4.8. File input: `read_file` and `to_string` (v0.0.28) — the two things a
  self-hosted compiler could not do without.
- A4.9. Guaranteed tail calls: `return tail f(...)` lowered to `musttail`
  (v0.0.29) — NOVELTY §4, the first novelty-register entry to ship.
- M4 phase 4b, part (v0.0.58): fields, record literals, builtins and enum
  constructors; false positives 24 of 88 and falling.
- M4 phase 4a (v0.0.57): the stage-1 typechecker — declarations, expressions,
  statements, the scale rules. Typechecks its own source. Plus `truncate(xs, n)`, which
  a scope-based checker cannot do without.
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
- FIX (v0.0.48): `expr_allocates` now sees through record literals, enum payloads and
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

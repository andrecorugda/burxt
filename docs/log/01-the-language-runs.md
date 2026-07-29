# The language runs

*Milestone log, v0.0.1 – v0.0.10. The design these versions serve is in [DESIGN.md](../../DESIGN.md); the whole log is indexed [here](README.md).*

Exact decimals with a declared scale, rounding contracts, functions, checked arithmetic, FFI, strings, structs, arrays. By the end of these ten, a Burxt program was a real native program.

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

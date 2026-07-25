# Burxt — Design Notes (v0.0.6)

**Burxt** is a typed, compiled programming language: exact decimals for money,
correctness by construction, native code through LLVM.

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
  widens with A4's types.
- A4. Strings, structs, collections <- NEXT
- A5. Refinement types ("balance >= 0", "splits sum to total")

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

# Burxt — Design Notes (v0.0.5)

## Grammar principle
The grammar must be eloquent and easy to understand, without compromising the
thesis. Types read as plain English (`Decimal<2, RoundHalfEven>` = "two
decimal places, rounding half to even"), there is one obvious way to write
each construct, and every compile error reads like advice — it names the rule
and shows the syntax that fixes it. When brevity and clarity conflict,
clarity wins; exactness and explicitness are never traded for either.

**Burxt** is a typed, compiled, native-Linux programming language.

## Thesis (what makes Burxt worth existing)
1. **Exact decimal is the DEFAULT numeric type for money.** No silent binary-float
   representation of currency. `Decimal<P,S>` carries precision + scale in the type.
2. **Correctness by construction.** Rounding must be explicit; float↔decimal mixing is a
   compile error. (Refinement types come later.)
3. **Native, no runtime baggage.** Compiles through LLVM to a native binary. No VM, no GC (yet).

## Compiler architecture (backend-independent front end)
Source (.bx)
  -> Lexer      (src/lexer.rs)      : text -> tokens
  -> Parser     (src/parser.rs)     : tokens -> AST (src/ast.rs)
  -> Typecheck  (src/typeck.rs)     : AST -> typed AST + errors
  -> Codegen    (src/codegen.rs)    : typed AST -> LLVM IR -> native object
  -> link       (cc)                : object -> executable

The front end (lexer/parser/typeck) knows NOTHING about LLVM. If we ever swap to
Cranelift or add an interpreter, only codegen.rs changes.

## Bootstrap plan
- Stage 0: this compiler, written in **Rust**, emitting via **LLVM 18** (inkwell).
- Stage 1 (future): rewrite the Burxt compiler in Burxt; compile it with stage 0.
  The day "Burxt compiles Burxt" = self-hosting = the language is real.

## v0.0.1 scope (the first vertical slice)
The smallest program that proves the thesis: exact decimal arithmetic with a
declared scale, printed exactly. Integers supported too, as the simplest path to
"it runs". Decimals are represented as scaled i64 (value * 10^scale) — exact, no float.

Example program (money.bx):
    let price: Decimal<2> = 19.99;
    let qty: Int = 3;
    let total: Decimal<2> = price * qty;
    print(total);        // 59.97  — exact, never 59.970000000001

## v0.0.2: rounding contracts
A rounding contract is an optional second type argument:

    Decimal<2>                 // no contract: only exact arithmetic
    Decimal<2, RoundHalfEven>  // ties to the even neighbor (banker's)
    Decimal<2, RoundHalfUp>    // ties away from zero (commercial)

Grammar principle: the type reads as plain English — "two decimal places,
rounding half to even" — and every rejection message shows the exact syntax
that fixes it.

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
- Division by zero traps at runtime (SIGFPE), like C — a checked story comes
  later; a silently wrong number is not an option.

Known i64 limits (until a wider representation lands): the double-scale
product `A*B` and the pre-scaled dividend `A*10^S` can overflow for values
near the top of the i64 range.

## Testing
`cargo test` runs a data-driven suite:
- tests/pass/NAME.bx + NAME.stdout — must compile & run with exactly that output.
- tests/fail/NAME.bx + NAME.stderr — must be rejected with an error containing that text.
Adding a test = dropping two files in the right directory.

## v0.0.3: functions, control flow, Bool
Burxt is now a real programming language: recursion makes it computationally
complete without needing mutation yet.

    fn total(price: Decimal<2>, qty: Int) -> Decimal<2> {
        return price * qty;
    }
    print(total(19.99, 3));    // 59.97

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

## v0.0.4: mutation and loops
Immutable is the default; mutation is opt-in and visible at the declaration:

    let mut b: Decimal<2, RoundHalfEven> = 1000.00;
    let mut m: Int = 0;
    while m < 12 {
        b = b * 1.01;      // contract applies at every step
        m = m + 1;
    }

Rules:
- `name = value;` only compiles for a `let mut` binding, and the value's type
  must match the declaration exactly. Parameters are immutable.
- `while` needs a Bool condition and braces, like `if`. A loop body never
  counts as "returns on every path" (the condition may be false at entry).
- Codegen: every alloca goes in the function's entry block, so a `let`
  inside a loop body cannot grow the stack per iteration.

## v0.0.5: checked arithmetic — no silently wrong numbers, ever
Every `+`, `-`, `*` (including the internal double-scale products behind
Decimal*Decimal and division) goes through `@burxt.checked.<op>`, built on
LLVM's `llvm.s{add,sub,mul}.with.overflow` intrinsics. On overflow the
program prints

    burxt runtime error: arithmetic overflow — the exact result no longer
    fits in the value range

to stderr and exits with code 70. Division by zero (and the lone
i64::MIN / -1 quotient) gets the same treatment — a named error instead of a
raw SIGFPE. This closes the last "silently wrong number" hole in the i64
representation; a wider representation can come later, but wraparound was
never acceptable.

The test suite gained a third category for this: tests/panic/*.bx must
compile but die at runtime with the expected message and a nonzero exit.

## Roadmap after v0.0.5
- refinement types ("balance >= 0", "splits sum to total")
- strings, arrays/records
- self-hosting

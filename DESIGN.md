# Burxt — Design Notes (v0.0.2)

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

## Roadmap after v0.0.2
- functions, control flow
- overflow-checked arithmetic (or a wider decimal representation)
- refinement types ("balance >= 0", "splits sum to total")
- self-hosting

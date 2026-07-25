# Burxt — Design Notes (v0.0.1)

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

## Roadmap after v0.0.1
- rounding contracts in the type (Decimal<S, RoundHalfEven>)
- functions, control flow
- refinement types ("balance >= 0", "splits sum to total")
- self-hosting

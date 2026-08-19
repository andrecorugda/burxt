---
layout: doc
title: Reference
section: reference
description: "Every keyword, builtin, operator and standard-library function — read out of the compiler rather than from memory."
---

{% raw %}

# Reference

Everything the language has. **Read out of the compiler**, not from memory: the keyword table below comes from `src/rust-compiler/lexer.rs`, the reserved names from `src/rust-compiler/typeck.rs`, the commands from `src/rust-compiler/main.rs`, and every standard-library entry from the `//` prose written above the declaration itself. `scripts/site-reference.py` regenerates these pages and a test diffs them, so this cannot fall behind the language again — which it had: the page this replaces still listed `record`, a keyword renamed eleven versions earlier.

<ul class="pages">
<li><a href="builtins.html"><span>Builtins</span> <span class="what">35 calls the language owns, and what each needs</span></a></li>
<li><a href="cli.html"><span>The command line</span> <span class="what">12 commands, including <code>review</code> and <code>mcp-schema</code></span></a></li>
<li><a href="option.html"><span><code>lib/option.bx</code></span> <span class="what">absence, made explicit</span></a></li>
<li><a href="result.html"><span><code>lib/result.bx</code></span> <span class="what">failure, made explicit</span></a></li>
<li><a href="decimal.html"><span><code>lib/decimal.bx</code></span> <span class="what">the helpers a money language is judged on</span></a></li>
<li><a href="string.html"><span><code>lib/string.bx</code></span> <span class="what">Strings, beyond the four builtins</span></a></li>
<li><a href="array.html"><span><code>lib/array.bx</code></span> <span class="what">the operations on a growable array that every program reaches for</span></a></li>
<li><a href="set.html"><span><code>lib/set.bx</code></span> <span class="what">membership, without a value nobody reads</span></a></li>
<li><a href="map.html"><span><code>lib/map.bx</code></span> <span class="what">a key-value table, in insertion order</span></a></li>
<li><a href="math.html"><span><code>lib/math.bx</code></span> <span class="what">integer arithmetic that does not lie about its edges</span></a></li>
<li><a href="fn.html"><span><code>lib/fn.bx</code></span> <span class="what">the four interfaces that stand in for a function value</span></a></li>
<li><a href="json.html"><span><code>lib/json.bx</code></span> <span class="what">JSON, parsed and rendered, in ordinary Burxt</span></a></li>
<li><a href="csv.html"><span><code>lib/csv.bx</code></span> <span class="what">comma-separated values, read and written, RFC 4180 with the</span></a></li>
<li><a href="html.html"><span><code>lib/html.bx</code></span> <span class="what">HTML as a typed tree, escaped at the one point it leaves</span></a></li>
<li><a href="cgi.html"><span><code>lib/cgi.bx</code></span> <span class="what">the request in, the response out, over the interface every web server has</span></a></li>
<li><a href="encoding.html"><span><code>lib/encoding.bx</code></span> <span class="what">hex, base64 and base64url, and every decoder REFUSES rather than guesses</span></a></li>
<li><a href="hash.html"><span><code>lib/hash.bx</code></span> <span class="what">hashes and checksums</span></a></li>
<li><a href="secure.html"><span><code>lib/secure.bx</code></span> <span class="what">bytes nobody can predict, and a comparison that does not leak</span></a></li>
<li><a href="vector.html"><span><code>lib/vector.bx</code></span> <span class="what">vector similarity, EXACTLY</span></a></li>
<li><a href="files.html"><span><code>lib/files.bx</code></span> <span class="what">files, without writing `external function fopen` yourself</span></a></li>
<li><a href="path.html"><span><code>lib/path.bx</code></span> <span class="what">POSIX paths, taken apart and put back together, lexically</span></a></li>
<li><a href="os.html"><span><code>lib/os.bx</code></span> <span class="what">the machine the program is running on</span></a></li>
<li><a href="net.html"><span><code>lib/net.bx</code></span> <span class="what">TCP, over the pointer wall</span></a></li>
<li><a href="http.html"><span><code>lib/http.bx</code></span> <span class="what">HTTP/1.1 over the sockets `net.bx` already opens</span></a></li>
<li><a href="tls.html"><span><code>lib/tls.bx</code></span> <span class="what">TLS by BINDING OpenSSL, which is the recorded decision rather than a shortcut</span></a></li>
<li><a href="time.html"><span><code>lib/time.bx</code></span> <span class="what">dates and durations, in whole seconds, in UTC</span></a></li>
<li><a href="random.html"><span><code>lib/random.bx</code></span> <span class="what">a SEEDED generator, and the name says seeded</span></a></li>
<li><a href="log.html"><span><code>lib/log.bx</code></span> <span class="what">four levels, a threshold from the environment, and stderr</span></a></li>
<li><a href="test.html"><span><code>lib/test.bx</code></span> <span class="what">testing Burxt, in Burxt</span></a></li>
<li><a href="https://bmx.burxt-lang.org/"><span><img class="eco-mark" src="../assets/bmx-wordmark.svg" alt="BMX"></span> <span class="what">the markup format — its own guide, spec and conformance suite</span></a></li>
<li><a href="https://star.burxt-lang.org/"><span><img class="eco-mark" src="../assets/starb-wordmark.svg" alt="star-burxt"></span> <span class="what">a front-end framework written in Burxt — a package you depend on, not part of the library</span></a></li>
</ul>

## Keywords

The 41 words the lexer knows. Every one of them is the word it means: `function`, not `fn`; `mutable`, not `mut`.

| | | | |
|---|---|---|---|
| `let` | `mutable` | `const` | `print` |
| `print_error` | `while` | `function` | `external` |
| `return` | `as` | `tail` | `pure` |
| `public` | `break` | `continue` | `if` |
| `else` | `true` | `false` | `class` |
| `private` | `region` | `enum` | `match` |
| `interface` | `is` | `self` | `implement` |
| `implements` | `for` | `in` | `dynamic` |
| `Int` | `Bool` | `String` | `CInt` |
| `CPointer` | `CDouble` | `Decimal` | `RoundHalfEven` |
| `RoundHalfUp` |  |  |  |

## Contextual markers

Recognised only where they appear — on a signature, inside a clause — so a variable may still be called `ensures` anywhere else.

| Marker | What it says |
|---|---|
| `allocates` | This function builds values in its CALLER's region. Optional and inferred since v0.0.142; writing it is still legal and still checked. |
| `requires` | A precondition. Checked, and it names itself when it fails. |
| `ensures` | A postcondition. `result` and `old(...)` are in scope inside one. |
| `decreases` | A measure that must fall on every recursive call, so the recursion ends. |
| `touches` | What the function may reach: `files`, `commands`, `clock`, `input`, `network`, `model`. It travels up the call chain. |
| `scaled` | In `as scaled`, at the C boundary — the only conversion that moves a Decimal across it. |
| `it` | The subject of a contract bracket: `Int [> 0, <= it * 2]`. Special ONLY inside a bracket. |

## Spellings that do not compile

Reserved so the compiler can name the replacement rather than say *unexpected identifier*. Each of these is an error that tells you the word to write instead.

| Written | Burxt spells it |
|---|---|
| `fn` | `function` |
| `mut` | `mutable` |
| `impl` | `implement` |
| `dyn` | `dynamic` |
| `extern` | `external` |
| `struct` | `class` |
| `trait` | `interface` |
| `record` | `class` |

## Types

| Type | What it is |
|---|---|
| `Int` | Signed 64-bit. **Traps** on overflow, never wraps |
| `Bool` | `true` / `false`. No coercion to or from anything |
| `String` | Bytes, NUL-terminated. `len` counts bytes |
| `Decimal<S>` | An integer scaled by `10^S`, exact. `S` up to 18 |
| `Decimal<S, R>` | The same, carrying a rounding contract: `RoundHalfEven` or `RoundHalfUp` |
| `[T; N]` | Fixed array. The length is part of the type. Bounds always checked |
| `[T]` | Growable array. Lives in a region. Bounds always checked |
| `dynamic Named` | An interface object: a value plus the interface's method table |
| `CInt`, `CDouble` | C's widths. Only in an `external function` signature |
| `i32`, `u8`, `u32`, `u64` | Sized C integers, boundary-only. A value that does not fit **traps** at the call rather than wrapping. `u64` is checked against `Int`'s signed maximum, because Burxt has no wider integer to receive the top half |

## Operators

| Operator | On | Notes |
|---|---|---|
| `+` `-` | Int, Decimal of the SAME scale, String (`+` only) | Scales must match. A rounding contract on one side is carried into the result; two different contracts are refused |
| `*` | Int, Decimal × Int, Decimal × Decimal | Mixed Decimal scales need a rounding contract |
| `/` | Decimal only | Always needs a rounding contract. `Int / Int` is refused — use `divide_floor` or `divide_toward_zero` |
| `==` `!=` | Any two values of the same type | One equality, no coercion. Strings compare by bytes |
| `<` `<=` `>` `>=` | Int, Decimal | Same type both sides. On String it is refused |
| `&&` `\|\|` | Bool | Short-circuit: the right side runs only if it is needed |
| `!` | Bool | |
| unary `-` | Int, Decimal | Checked, like every other arithmetic |
| `+=` `-=` `*=` | Anything the long form allows | `x += e` IS `x = x + e` by the time it is checked. Works on a name, a field and an array element. There is deliberately no `++` |
| `?` | `Result<T, E>` | `f(x)?` returns the failure unchanged, or unwraps the success |

## Shorthands

The whole list. Burxt has few on purpose, and each is sugar over something it already means rather than a second way to mean it.

| Written | Means |
|---|---|
| `$19.99` | `19.99` as a `Decimal<2>`. The `$` is documentation, not arithmetic |
| `8.25%` | `0.0825` as a `Decimal<4>` — a percent is two scales finer than the number it is written as |
| `"total: {amount}"` | `"total: " + to_string(amount)` — and inside a `print`, the pieces go out in order with nothing built |
| `x += e` | `x = x + e`. Also `-=` and `*=`, on a name, a field, or `xs[i]` |
| `P { x, y }` | `P { x: x, y: y }` — a field taking a variable of the same name |
| `f();` | a call kept for its effect, with no binding |
| `function (self) m()` | `function (self: Type) m()`, inside a block whose header already said which type |
| `P { x: 1, }` | a trailing comma, anywhere a list is written, so adding an item is a one-line diff |
| `let x = e;` | `let x: T = e;` where `T` is `e`'s type. Arrays are the exception — a literal does not say fixed or growable |
| `for x in xs { }` | `let mutable i = 0; while i < len(xs) { let x = xs[i]; … }` — `xs` must be a name or a field path |
| `for i in 0..n { }` | `let mutable i = 0; while i < n { … }` — the end is **exclusive**, `i` is immutable, and both bounds are Ints evaluated once. There is no `..=`, and a range is not a value |
| `f(x)?` | `match f(x) { Error(e) => return Error(e), Ok(v) => v }` — the failure variant found by name, never converted |
| `Int [> 0, <= n]` | `requires` clauses written on the value instead of under the signature |

## What stops a running program

| What happens | When |
|---|---|
| `arithmetic overflow — the exact result no longer fits in the value range` | `+` `-` `*` on an `Int` or a `Decimal` past its range |
| an index out of range | `xs[i]` outside `0 .. len(xs) - 1` |
| a broken precondition, quoted back with the clause that failed | a `requires` that does not hold at a call |
| a broken postcondition | an `ensures` that does not hold at a return |
| a `decreases` measure that did not decrease | a recursion that would not have ended |
| dividing by zero | `divide_floor`, `divide_toward_zero`, `remainder` |
| `cannot cross as a C double exactly` | a value going out through `CDouble` that a double cannot hold |

Every one of them ends the program, and every one names itself. Nothing here is a warning, and
nothing continues with a wrong value.

## Not present, each for a reason

Block comments (`/* … */`) — line comments only, so there is no nesting rule to get wrong. Multi-line string literals — a literal spanning lines makes its own indentation part of the data, which is the one thing that surprises everybody about them; use `\n` and `+`. `for i in 0..n` — a range is a second construct, and `while i < n` says it. `for x in text` — a String is bytes, and `byte_at` says BYTE so the byte-versus-character question cannot hide. `x++` — an expression with a side effect. `a ? b : c` — `if` as an expression would make it redundant. Arrow functions — they are closures, and captured state needs an owner.


{% endraw %}

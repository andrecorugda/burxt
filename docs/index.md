---
layout: default
title: null
description: A typed, compiled, native language where exact decimals are the default and correctness is enforced by the compiler.
width: wide
---

<div class="hero">

<img class="lockup" src="{{ site.baseurl }}/assets/burxt-lockup-light.png" alt="Burxt">

<p class="line">A typed, compiled, native language where exact decimals are the default
and correctness is enforced by the compiler — not left to discipline.</p>

<pre><code>print("Hello, world!");</code></pre>

<p class="line" style="font-size:16px; margin-top:1rem;">That is a complete program.
There is no entry point to declare.</p>

<div class="cta">
  <a class="btn" href="{{ site.baseurl }}/install/">Install</a>
  <a class="btn ghost" href="{{ site.baseurl }}/guide/">Read the guide</a>
</div>

</div>

<div class="wrap">

## Money is not a float

Most languages hand you binary floating point and trust you to remember it cannot hold `0.10`.
Burxt makes the exact type the default and the inexact one impossible to reach by accident.

```burxt
let price: Decimal<2> = 19.99;
let qty:   Int        = 3;
let total: Decimal<2> = price * qty;
print(total);            // 59.97 — computed as scaled integers, no float anywhere
```

A `Decimal<2>` is an integer and a scale. Adding two decimals of different scales is a **compile
error**, not a silent rounding. Multiplying where the result would narrow requires you to name the
rounding rule — because someone has to decide, and it should not be the compiler behind your back.

## What it refuses

The list is the design. Every one of these is a compile error rather than a habit you have to
remember:

<div class="tablewrap" markdown="1">

| | |
|---|---|
| **No null** | Absence is a type. `Option<T>` is a library, not a keyword, and `match` forces both cases |
| **No silent overflow** | `+` on `Int` traps. A money value never wraps around quietly |
| **No implicit coercion** | An `Int` is not a `Decimal` is not a `Bool`. You convert, or you do not |
| **No binary floats for money** | Exact decimals are the default; scales must match |
| **No garbage collector** | Regions: a bump pointer and a mark. Release is O(1), whatever you allocated |
| **No inheritance** | Traits and composition. No fragile base class, no constructor order to remember |
| **No hidden allocation** | A function that builds in your region says `allocates` in its signature |

</div>

## It compiles itself, and the two compilers agree

The Burxt compiler is written in Burxt — 8,300 lines of lexer, parser, typechecker and LLVM-IR
backend — and it compiles its own source. The compiler *it* produces emits **byte-identical** output
for that same source.

That fixpoint is the strongest claim this project makes. It says the two independent
implementations — one in Rust, one in Burxt — agree about the *whole language*, not merely about the
programs someone thought to test. Every push checks it.

## Native, and small

A compiled Burxt program is a native executable of about **16 KB** that links nothing but libc. The
allocator, the string operations and the overflow checks are emitted into every module, so there is
no runtime to ship and no VM to start.

<div class="cta" style="justify-content:flex-start; margin: 2.5rem 0 0;">
  <a class="btn" href="{{ site.baseurl }}/examples/">See it doing something</a>
  <a class="btn ghost" href="https://codespaces.new/andrecorugda/burxt?quickstart=1">Try it in a browser</a>
</div>

<p style="color:var(--ink-soft); font-size:14px; margin-top:2.5rem;">
Burxt is early. It is not ready for production — it is ready to try, read and shape.
</p>

</div>

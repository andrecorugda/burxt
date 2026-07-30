---
layout: default
title: null
description: A typed, compiled, native language whose signatures carry the promises — so a reviewer can see what a change can do, and the compiler refuses what it cannot.
width: wide
---

<div class="hero" markdown="1">

<img class="lockup" src="{{ site.baseurl }}/assets/burxt-lockup-light.png" alt="Burxt">

<p class="line">A typed, compiled, native language where the <strong>signature carries the
promise</strong> — the scale, the rounding, the preconditions, what it can reach — so you can tell
whether code is right by reading its declarations.</p>

<pre><code>function withdraw(balance: Decimal&lt;2&gt;, amount: Decimal&lt;2&gt;) -&gt; Decimal&lt;2&gt;
    requires amount &gt; $0.00
    requires amount &lt;= balance
{
    return balance - amount;
}</code></pre>

<p class="line" style="font-size:16px; margin-top:1rem;">Exact money, a precondition the compiler
enforces, and nothing hidden in the body. That is a complete program — there is no entry point to
declare.</p>

<div class="cta">
  <a class="btn" href="{{ site.baseurl }}/refused/">See what it refuses</a>
  <a class="btn ghost" href="{{ site.baseurl }}/guide/">Read the guide</a>
  <a class="btn ghost" href="{{ site.baseurl }}/install/">Install</a>
</div>

</div>

<div class="wrap" markdown="1">

## Most code is now read more than it is written

Reviewing a change means answering one question: *can this do something it could not do before?*

In most languages you answer it by reading every line, because anything important can hide inside
a body — an assertion someone deleted, a network call someone added, a field someone stopped
protecting. Burxt puts those in the signature instead, which makes the question answerable:

```
$ burxt review before.bx after.bx
WEAKENED  Account.withdraw   lost `requires amount <= self.balance`
WEAKENED  invoice_total      now touches network — it could not before
WEAKENED  Account.balance    no longer `private` — anything may now read it
STRICTER  line_tax           gained `requires quantity > 0`
```

A deleted precondition is the most dangerous change anyone can make and the hardest to notice: it
passes every test, because the tests were failing *because of it*. Here it is a change to a
declaration, so a tool can find it. **`burxt review` exits non-zero when a promise gets weaker** —
which makes it a gate rather than a report.

## Ten mistakes that compile everywhere else

<div class="tablewrap" markdown="1">

| | |
|---|---|
| `Decimal<2> + Decimal<4>` | a rate added to a price |
| `subtotal * rate` into a `Decimal<2>` | six exact places quietly becoming two |
| a total past what an `Int` holds | wraps to a negative in every other language |
| `Account { balance: ... }` | the constructor, and its checks, skipped |
| a `match` written before a variant existed | falls through, silently |
| a `String` from a model, used as money | |

</div>

Each is code that type-checks in Python, runs in PHP, and passes review because nothing about it
looks wrong. [**Read all ten with the compiler's exact words**]({{ site.baseurl }}/refused/) — then
ask which you would have caught at 5pm on a Friday.

## Money is exact, because that is where being wrong costs

```burxt
let price: Decimal<2> = 19.99;
let quantity: Int     = 3;
let total: Decimal<2> = price * quantity;
print(total);            // 59.97 — scaled integers, no float anywhere
```

A `Decimal<2>` is an integer and a scale, both in the type. Adding two different scales is a
**compile error**, not a silent rounding. Narrowing a product makes you name the rounding rule —
`Decimal<2, RoundHalfUp>` — and that rule then travels through every signature the value reaches,
so a reviewer sees it without opening another file.

## What it refuses

The list is the design. Every one is a compile error rather than a habit you have to remember:

<div class="tablewrap" markdown="1">

| | |
|---|---|
| **No null** | Absence is a type. `Option<T>` is a library, not a keyword, and `match` forces both cases |
| **No silent overflow** | `+` on `Int` traps. A money value never wraps around quietly |
| **No implicit coercion** | An `Int` is not a `Decimal` is not a `Bool`. You convert, or you do not |
| **No unstated effects** | A function says what it `touches` — files, commands, network — or it may not reach them |
| **No hidden allocation** | Building a value needs somewhere to go, and the compiler works out where |
| **No garbage collector** | Regions: a bump pointer and a mark. Release is O(1), whatever you allocated |
| **No inheritance** | Interfaces and composition. No fragile base class, no constructor order |

</div>

## Familiar on purpose

`class`, `interface`, `implements`, `private`, a constructor, `match` on a value. If you write PHP
or C#, you can read this today:

```burxt
class Account {
    owner: String,
    private balance: Decimal<2>,

    function open(owner: String, opening: Decimal<2>) -> Account
        requires opening >= $0.00
    {
        return Account { owner: owner, balance: opening };
    }
}
```

An unfamiliar spelling is something a reader has to stop and decode, and that cost is paid on every
review. So the vocabulary is deliberately the one most people already have.

## It compiles itself, and the two compilers agree

The Burxt compiler is written in Burxt — 8,300 lines of lexer, parser, typechecker and LLVM-IR
backend — and it compiles its own source. The compiler *it* produces emits **byte-identical** output
for that same source.

That fixpoint is the strongest claim here. It says two independent implementations — one in Rust,
one in Burxt — agree about the *whole language*, not merely about the programs someone thought to
test. Every push checks it.

A compiled program is a native executable of about **16 KB** linking nothing but libc. No runtime to
ship, no VM to start.

<div class="cta" style="justify-content:flex-start; margin: 2.5rem 0 0;">
  <a class="btn" href="{{ site.baseurl }}/refused/">See what it refuses</a>
  <a class="btn ghost" href="{{ site.baseurl }}/examples/">See it doing something</a>
  <a class="btn ghost" href="https://codespaces.new/andrecorugda/burxt?quickstart=1">Try it in a browser</a>
</div>

<p style="color:var(--ink-soft); font-size:14px; margin-top:2.5rem;">
Burxt is early. It is not ready for production — it is ready to try, read and shape.
</p>

</div>

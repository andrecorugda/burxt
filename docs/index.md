---
layout: default
title: null
description: A typed, compiled, native language for the way code gets written now — an agent writes it, you scan it, and a mistake costs money. Strict enough that the agent cannot make one, plain enough that you can see it didn't.
width: wide
no_roam: true
---

<div class="hero" markdown="1">

<h1 class="vh">Burxt</h1>

<div class="lockup-live">
<picture class="mascot">
<source media="(prefers-reduced-motion: reduce)" srcset="{{ site.baseurl }}/assets/burxt-ember-still.png">
<img class="mark" src="{{ site.baseurl }}/assets/burxt-ember.gif" width="174" height="222" data-replay
     alt="The Burxt mark — an ember hops out from behind the b, looks around, waves, and hops back in">
</picture>
<img class="word" src="{{ site.baseurl }}/assets/burxt-wordmark.png" width="823" height="217" alt="burxt">
</div>

<p class="line"><strong>Strict enough that an agent cannot make a costly mistake.<br>
Plain enough that you can see that it didn't.</strong></p>

<p class="line" style="font-size:17px;">An agent writes the code now. You scan it and approve it —
you do not read every line. So Burxt puts the things that can cost you money where you are already
looking: the scale, the rounding, the preconditions, what a function is allowed to reach.</p>

<p class="line" style="font-size:16px;">If you write PHP or C#, you can read this today.</p>

<pre><code>class Account {
    owner: String,
    private balance: Decimal&lt;2&gt;,

    function open(owner: String, opening: Decimal&lt;2&gt;) -&gt; Account
        requires opening &gt;= $0.00
    { return Account { owner: owner, balance: opening }; }

    function (self) withdraw(amount: Decimal&lt;2&gt;) -&gt; Account
        requires amount &gt; $0.00
        requires amount &lt;= self.balance
    { return Account { owner: self.owner, balance: self.balance - amount }; }

    function (self) shown() -&gt; String { return self.owner + ": " + to_string(self.balance); }
}

let ada: Account = Account.open("ada", $100.00);
print(ada.withdraw($30.00).shown());          // ada: 70.00</code></pre>

<p class="line" style="font-size:16px; margin-top:1rem;">That is the whole file — no entry point to
declare. And <code>balance</code> cannot go negative: not <em>should not</em>, <strong>cannot</strong>.
<code>open</code> is the only way to make an Account, <code>withdraw</code> the only way to change
one, and nothing outside the class may name <code>balance</code> in a literal to get around either.
You can confirm all of that from the declarations, without reading a body.</p>

<div class="cta">
  <a class="btn" href="{{ site.baseurl }}/refused/">See what it refuses</a>
  <a class="btn ghost" href="{{ site.baseurl }}/guide/">Read the guide</a>
  <a class="btn ghost" href="{{ site.baseurl }}/install/">Install</a>
  <a class="btn ghost" href="{{ site.baseurl }}/limitations.html">What it does not do</a>
  <a class="btn ghost" href="{{ site.baseurl }}/compatibility.html">What it promises</a>
</div>

</div>

<div class="wrap" markdown="1">

## The change you are reviewing at 5pm

An agent could not satisfy a rule, so it deleted the rule. Nobody argued with it. One line, in a
diff of forty.

**Every test still passes.** More of them pass than before, because whatever was failing was failing
*on purpose*. There is no warning, and nothing in that diff looks different from any other deleted
line — in every other language an assertion in a body **is** just another line in a body.

That is the single most dangerous change anyone can make to a program, and it is the one this
language exists to catch. Because the rule lives in the signature, a tool can see it go:

```
$ burxt review before.bx after.bx
WEAKENED  Account.balance                    no longer `private` — anything may now read it
WEAKENED  Account.withdrawn                  lost `requires amount <= self.balance`
WEAKENED  invoice_total                      now touches network — it could not before
STRICTER  line_tax                           gained `requires quantity > 0`

3 weakened promise(s). A weakened contract is the one change that passes every test — the tests were failing BECAUSE of it.
```

**It exits non-zero when a promise gets weaker**, so it is a gate and not a report. Put it in CI and
nothing can quietly promise less than it did yesterday — not a deleted precondition, not a function
that started reaching the network, not a field that stopped being private.

This works for one reason, and it is the reason for every other decision here: **everything that
matters is in the signature.** An agent reasons one function at a time and you scan; neither of you
has the whole program in view. So a tool that reads only declarations can still answer the only
question a review is really asking.

## Six mistakes an agent makes confidently

<div class="tablewrap" markdown="1">

| | |
|---|---|
| `Decimal<2> + Decimal<4>` | a rate added to a price |
| `subtotal * rate` into a `Decimal<2>` | six exact places quietly becoming two |
| a total past what an `Int` holds | wraps to a negative in every other language |
| `Account { balance: ... }` | the constructor, and its checks, skipped |
| a `match` written before a variant existed | falls through, silently |
| a `String` from a model, used as money | the one number nothing verified, spent |

</div>

Every one of those type-checks in Python, runs in PHP, and passes review — because nothing about any
of them *looks* wrong. They are not the mistakes of a careless writer; they are the mistakes of a
confident one working from a local view.

In Burxt each is a **compile error**, which means it is a category of mistake permanently removed
from your job rather than one more thing to watch for.
[**Read all ten, with the compiler's exact words**]({{ site.baseurl }}/refused/) — then ask honestly
which of them you would have caught at 5pm on a Friday.

## Money, because that is where being wrong is most expensive

```burxt
let price: Decimal<2> = 19.99;
let quantity: Int     = 3;
let total: Decimal<2> = price * quantity;
print(total);            // 59.97 — scaled integers, no float anywhere
```

A `Decimal<2>` is an integer and a scale, both in the type — no float anywhere. Adding two different
scales is a **compile error**, not a silent rounding. Narrowing a product makes you name the rounding
rule — `Decimal<2, RoundHalfUp>` — and that rule then travels through every signature the value
reaches, so you see how a total rounds without opening another file.

Exact money is not the point of Burxt; it is the **clearest demonstration** of the point. Money is
where a plausible wrong answer costs the most, which is why the flagship example is a till. A version
of this language that got decimals perfectly right and still let an agent ship a believable wrong
program would have failed at what it is for.

## What it refuses

The list is the design, and the economy behind it is one sentence: **every compile error is a review
you do not have to do.** A refusal is not friction — it is a category of mistake that can no longer
reach you.

<div class="tablewrap" markdown="1">

| | |
|---|---|
| **No null** | Absence is a type. `Option<T>` is a library, not a keyword, and `match` forces both cases |
| **No silent overflow** | `+` on `Int` traps. A money value never wraps around quietly |
| **No implicit coercion** | An `Int` is not a `Decimal` is not a `Bool`. You convert, or you do not |
| **No unstated effects** | A function says what it `touches` — files, commands, network — or it may not reach them |
| **No garbage collector** | A region is a bump pointer and a mark. Release is O(1), whatever you put in it — and you write nothing to get it |
| **No inheritance** | Interfaces and composition. No fragile base class, no constructor order |

</div>

## The tool an agent calls, and its schema, are one thing

An MCP tool ships a **JSON Schema** saying what may be passed to it. The function behind it also
checks what it was passed. Those are one fact written twice, and everywhere else they drift — the
schema keeps a bound the code relaxed a year ago, the client sends something valid *by the schema*,
the tool refuses it, and the failure arrives as a 500 instead of as a validation message.

Here the precondition is in the signature, so there is nothing to keep in step:

```burxt
function line_total(unit: Decimal<2> [> $0.00], quantity: Int [> 0, <= 100000]) -> Decimal<2> {
    return unit * quantity;
}
```

`burxt mcp-schema` reads that declaration and answers:

```json
{"name":"line_total","inputSchema":{"type":"object","properties":{
  "unit":     {"type":"string","description":"Decimal<2>","exclusiveMinimum":"0.00"},
  "quantity": {"type":"integer","description":"Int","exclusiveMinimum":"0","maximum":"100000"}},
  "required":["unit","quantity"]}}
```

**The schema cannot drift from the implementation, because there is only one of it.** No other
language can do this, and the reason is not tooling — it is that no other language puts the contract
in the signature. In Python or TypeScript it is a decorator or a separate object, and keeping the two
in step is a code review, which is precisely the review this language exists to remove.

The money half matters as much. A tool that returns an amount sends it as a **quoted string**, because
a JSON number reaches a JavaScript consumer as a double and loses the cent. And `19.999` asked for as
a `Decimal<2>` is **refused, never rounded** — the caller sent a third decimal place for a reason.

[**How it works, and what it cannot express**]({{ site.baseurl }}/guide/12-tools-and-agents.html) —
including the honest part: a clause relating two parameters has no key in JSON Schema, so it is
skipped and *reported on stderr* rather than approximated.

## Familiar on purpose — which is a safety property, not a preference

`class`, `interface`, `implements`, `private`, a constructor, `match` on a value. Nothing here needs
a new mental model:

```burxt
interface Priced {
    function price(self) -> Decimal<2>
}

class Meal implements Priced {
    dish: String,
    cost: Decimal<2>,
    function (self) price() -> Decimal<2> { return self.cost; }
}

function label(status: Int) -> String {
    match status {
        1 => { return "paid"; }
        2 => { return "owing"; }
        _ => { return "unknown"; }
    }
}
```

An unfamiliar spelling is a thing **you** have to stop and decode on every review, and a thing the
**agent** has to have memorised correctly. Both costs are paid every single time. So the vocabulary is
deliberately the one the 70% who write PHP and C# already have — and where that meant undoing a
borrowed idea, it got undone.

## It compiles itself, and the two compilers agree

The Burxt compiler is written in Burxt — 9,900 lines of lexer, parser, typechecker and LLVM-IR
backend — and it compiles its own source. The compiler *it* produces emits **byte-identical** output
for that same source.

That fixpoint is the strongest claim here. It says two independent implementations — one in Rust,
one in Burxt — agree about the *whole language*, not merely about the programs someone thought to
test. Every push checks it.

A compiled program is a native executable of about **16 KB** linking nothing but libc. No runtime to
ship, no VM to start.

## What this is asking of you

Nothing about your review habits, which is the point. Keep scanning declarations — that is what you
already do when a diff is longer than your afternoon. The difference is that here the declarations
are load-bearing: a promise cannot be quietly smaller than it was, a number cannot quietly change
scale, and a function cannot quietly start reaching the network.

The work you stop doing is checking for the mistakes on this page.

<div class="cta" style="justify-content:flex-start; margin: 2.5rem 0 0;">
  <a class="btn" href="{{ site.baseurl }}/refused/">See what it refuses</a>
  <a class="btn ghost" href="{{ site.baseurl }}/examples/">See it doing something</a>
  <a class="btn ghost" href="https://codespaces.new/andrecorugda/burxt?quickstart=1">Try it in a browser</a>
</div>

<p style="font-size:14px; margin-top:2.5rem;">
Burxt is early. It is not ready for production — it is ready to try, read and shape.
</p>

</div>

---
title: Practices
description: Where a contract belongs and where it does not, how to build a string without making it quadratic, and the traps this project has already paid for.
---

# 13. Practices

## What this is for
{: #what-this-is-for}

The rest of this guide teaches what the language *is*. This page is about using it well.

Every rule below was paid for, most of them by this compiler. The string-building one cost eleven
versions of a quadratic lexer and 1,132 MB of peak memory. The absence one was a library function
that answered `""` for a file that was not there.

**Every snippet on this page is compiled by the test suite**, including the ones shown being
refused. A practices page whose examples do not compile is worse than none.

## Think of a load-bearing wall
{: #think-of-a-load-bearing-wall}

A building has two kinds of wall, and everyone on site knows which is which before they pick up a
hammer.

A **load-bearing** wall holds the building up. It is where it is because of physics, it is the same
on every floor and in every tenancy, and you do not move it because a tenant would prefer the room
wider.

A **partition** is where it is because somebody decided. It can move next year, it can differ
between two floors of the same building, and moving it is an ordinary day's work rather than an
engineering review.

<figure>
<svg viewBox="0 0 680 240" style="width:100%; max-width:100%; height:auto;" role="img" aria-label="Two walls: a load-bearing wall marked structural and immovable, and a partition marked movable — the contract and the validator">
  <style>
    .wall  { fill: #1d1d1f; }
    .part  { fill: #ffffff; stroke: #1d1d1f; stroke-width: 2; stroke-dasharray: 6 4; }
    .floor { fill: none; stroke: #1d1d1f; stroke-width: 2; }
    .hair  { fill: none; stroke: #d2d2d7; stroke-width: 1; }
    .h     { font: 600 13.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .t     { font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #1d1d1f; }
    .cap   { font: 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .mut   { font: 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #6e6e73; }
    .arr   { fill: none; stroke: #6e6e73; stroke-width: 1.6; stroke-linecap: round; }
  </style>

  <line class="floor" x1="40" y1="180" x2="640" y2="180"/>
  <line class="hair"  x1="40" y1="40"  x2="640" y2="40"/>

  <rect class="wall" x="150" y="40" width="16" height="140"/>
  <text class="h"   x="110" y="205">load-bearing</text>
  <text class="t"   x="96"  y="223">requires / ensures</text>
  <text class="cap" x="196" y="70">true for every call, forever</text>
  <text class="mut" x="196" y="90">moving it is a major version</text>

  <rect class="part" x="470" y="70" width="12" height="110"/>
  <text class="h"   x="440" y="205">partition</text>
  <text class="t"   x="424" y="223">Result&lt;T, E&gt;</text>
  <text class="cap" x="326" y="70">true today, for this tenant</text>
  <text class="mut" x="326" y="90">moving it is Tuesday</text>
  <path class="arr" d="M455 125 L500 125"/>
  <path class="arr" d="M500 125 l-7 -5 M500 125 l-7 5"/>
</svg>
<figcaption>A contract holds the building up. A validator is where somebody put it.</figcaption>
</figure>

Put a rule in the wrong wall and you get one of two failures. A partition made load-bearing is a
`requires` that stops being true when the configuration changes — a signature that lies exactly when
somebody edits a config file. A load-bearing wall made a partition is a real invariant left to a
runtime check that a caller can forget.

## A step closer
{: #a-step-closer}

There is one question, and it decides every case:

> **Could this rule be answered differently by two runs of the same program?**

If yes, it is a partition. It goes in the body and comes back as a `Result`, because the caller has
to be able to be told no.

If no — if it is true of the function itself, for every caller, on every machine, forever — it is
load-bearing and it goes in the signature.

"A withdrawal amount must be positive" is not a business rule that a tenant configures. It is what
the word *withdrawal* means. "A withdrawal must be under the daily limit" is a number in a database.

## In code
{: #in-code}

```burxt
// LOAD-BEARING. There is no configuration that makes a negative withdrawal correct.
function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>
    requires amount > $0.00
    requires amount <= balance
    ensures result >= $0.00
{
    return balance - amount;
}

print(withdraw($100.00, $30.00));
```

```burxt
// PARTITION. The cap comes from somewhere, it changes, and a caller must be able to handle no.
use "lib/result.bx";

function check_limit(amount: Decimal<2>, daily_cap: Decimal<2>) -> Result<Decimal<2>, String> {
    if amount > daily_cap {
        return Result.Error("over the daily limit");
    }
    return Result.Ok(amount);
}

match check_limit($900.00, $500.00) {
    Error(why) => { print(why); }
    Ok(amount) => { print(amount); }
}
```

**Contracts compose, through `pure` functions.** You do not repeat a rule twenty times — a rule
library is ordinary code:

```burxt
pure function is_positive(amount: Decimal<2>) -> Bool {
    return amount > $0.00;
}

pure function within(amount: Decimal<2>, cap: Decimal<2>) -> Bool {
    return amount <= cap;
}

function transfer(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>
    requires is_positive(amount)
    requires within(amount, balance)
{
    return balance - amount;
}

print(transfer($100.00, $30.00));
```

The failure names the clause you wrote, not its expansion:

```
burxt runtime error: `requires within(amount, balance)` failed in `transfer`
```

`pure` is what makes that safe. A predicate that could read a file would be a side effect running at
every call, in every build mode. It is refused:

```burxt
pure function looks_ok(n: Int) -> Bool {
    print(n);
    return n > 0;
}
```

```
error: `pure function looks_ok` may not print: a pure function's result must depend only on its arguments
```

## Why it is built this way
{: #why-it-is-built-this-way}

**There is no contract builder, and there will not be one.**

A query builder makes a value you then run. A contract is never run as a value — it is *part of the
signature*, and three things depend on that:

- **`burxt review` diffs contracts between versions.** A clause assembled at runtime is not in the
  interface, so it cannot be compared — and the semver rule is built on that comparison.
- **`burxt mcp-schema` derives a tool schema from `requires`.** It reads the signature.
- **A reader sees what a function demands without running it.** `requires rules.build()` tells them
  nothing, which is the whole thing this language is for.

It would also need closures, and those were declined for the same reason: a function value hides its
captured state from the signature.

## What it costs
{: #what-it-costs}

Three real costs, stated rather than discovered.

**A rule that varies has to be written twice** — once as the validator, once as the contract for the
part that does not vary. That is more typing than a single configurable check, and it is the price
of the signature staying true.

**There are no lambdas**, so a `map` costs a class and two bindings:

```burxt
use "lib/array.bx";
use "lib/fn.bx";

class Times { by: Int }
implement Mapper<Int, Int> for Times {
    function (self) apply(x: Int) -> Int { return x * self.by; }
}

let xs: [Int] = [1, 2, 3];
let times: Times = Times { by: 10 };
let scale: dynamic Mapper<Int, Int> = times;
print(array_map(xs, scale)[2]);
```

Note the two bindings: an interface object borrows the value behind it, so it must come from a named
variable rather than a temporary.

**And the compiler will not save you from a quadratic.** `out = out + piece` in a loop copies the
whole string every pass:

```burxt
// WRONG, and it looks fine until the input grows. No test will tell you.
function joined_slowly(pieces: [String]) -> String {
    let mutable out: String = "";
    let mutable i: Int = 0;
    while i < len(pieces) {
        out = out + pieces[i];
        i += 1;
    }
    return out;
}

let words: [String] = ["a", "b", "c"];
print(joined_slowly(words));
```

Use `string_join`, which halves the list pairwise instead. And `len(s)` walks the string, so measure
it once outside a loop rather than in the condition:

```burxt
function count_spaces(text: String) -> Int {
    let n: Int = len(text);
    let mutable spaces: Int = 0;
    let mutable i: Int = 0;
    while i < n {
        if byte_at(text, i) == 32 {
            spaces += 1;
        }
        i += 1;
    }
    return spaces;
}

print(count_spaces("a b c"));
```

## When you reach for it
{: #when-you-reach-for-it}

| the question | the answer |
|---|---|
| always true, every caller, forever | `requires` / `ensures` |
| true today, for this tenant, from a database | a validator returning `Result` |
| the same rule in many signatures | a `pure` predicate, called from the clause |
| the caller could carry on without it | `Option`, never a value that looks fine |
| the caller could not carry on | end the program — named, exit 70 |
| a transformation named and reused | an interface object |
| a one-off transformation | write the loop |
| something a dependent package may call | `public` — a promise for a major version |

**Two naming habits worth copying**, both from the standard library. Say the limit in the name —
`string_to_upper_ascii`, `divide_floor`, `string_equals_constant_time`, `random_from(seed)` — because
a `to_upper` that silently mangles non-ASCII is the quiet wrong answer this language exists to
refuse. And declare the narrowest effect that is true: `touches` propagates to every caller, so a
wide declaration is not more permissive, it is less informative.

## Examples
{: #examples}

**Absence, and which of the two you are writing:**

```burxt
use "lib/files.bx";

// `file_read` ENDS the program if the file is missing — right for a config file the program
// cannot run without. `file_read_maybe` answers None for missing, unreadable AND a directory.
match file_read_maybe("settings.txt") {
    None => { print("no settings; using the defaults"); }
    Some(text) => { print(len(text)); }
}
```

**Exposing a package:**

```burxt
// Reachable by anyone who depends on this package — and a promise you keep for a major version.
public function tax_of(amount: Decimal<2>, rate_cents: Int) -> Decimal<2> {
    return amount + $0.01 * rounded(rate_cents);
}

// Not. It can change in a patch without breaking anyone.
function rounded(n: Int) -> Int {
    return n;
}

print(tax_of($100.00, 7));
```

**Testing.** A passing test cannot tell "supported" from "never examined", so every refusal gets a
fixture that is expected to fail, with the reason in a comment. And cover the *shape*, not one
instance of it: a fixture for "a tuple of two slices from a generic" existed for a hundred versions
using `[Int]`. The same shape with `[String]` crashed the compiler, and the suite was green
throughout.

## Next
{: #next}

[Back to the guide](index.md) — or [what Burxt does not do]({{ site.baseurl }}/limitations.html),
which is the other half of knowing how to use it.

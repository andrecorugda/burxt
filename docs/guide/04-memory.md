---
title: Memory
---

# 4. Memory

## The problem, as it actually arrives

Every language answers *"when does this storage go away?"*, and there are only two ways to get it
wrong.

**Free it too late** and you have a garbage collector. Nothing is ever wrong, exactly — it is just
that once a month, at the worst possible moment, the program stops for 400ms and nobody can say why.

**Free it too early** and you have the outcome this whole language is built against. Here is a real
one, from Burxt's own compiler:

```burxt
function secret(tag: Int) -> String {
    region inner {
        let s: String = "secret-" + to_string(tag);
        return s;                        // `inner` releases before this returns
    }
}
```

```
error: cannot return this String: it was built inside a `region` block, which releases at
       its closing brace, so its storage would not outlive the call.
```

That is what happens today. For four versions it **compiled**. It did not crash either — it printed
an **empty string**: plausible, harmless-looking, and wrong. Every test around it was green, because
nothing asserted on a value nobody suspected.

A use-after-free that segfaults is a bad afternoon. A use-after-free that returns *believable bytes*
is a bug that ships. Burxt refuses that code now, and the rest of this page is the one idea that
makes refusing it cheap.

## Think of a tray

Your program has a tray. Everything it builds — a joined String, a `to_string`, a `substring`, a
file it read, a growable array — goes on the tray. When you are done with a batch of work, you tip
the **whole tray** into the bin in one motion. You never pick items off it one at a time.

No collector, because nothing needs finding. No reference counts, because nothing is counted. No
lifetimes to prove, because the tray is the lifetime.

## What the tray is, in memory

<svg viewBox="0 0 640 222" role="img" aria-label="A region is a bump pointer and a mark; closing it puts the pointer back" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .arena { fill: none; stroke: #111; stroke-width: 1.5; }
    .b { fill: #fff; stroke: #111; stroke-width: 1.2; }
    .r { fill: #fff; stroke: #b00; stroke-width: 1.5; stroke-dasharray: 4 3; }
    .tick { stroke: #111; stroke-width: 2.5; }
    .t { font: 11px ui-monospace, monospace; fill: #111; }
    .g { font: 11px ui-monospace, monospace; fill: #888; }
    .s { font: 12px ui-monospace, monospace; fill: #b00; }
    .a { stroke: #b00; stroke-width: 1.8; fill: none; marker-end: url(#a4); }
    @media (prefers-color-scheme: dark) {
      .arena { stroke: #ddd; } .b { fill: #1b1b1b; stroke: #ddd; } .r { fill: #1b1b1b; stroke: #ff8080; }
      .tick { stroke: #ddd; } .t { fill: #eee; } .s { fill: #ff8080; }
      .a { stroke: #ff8080; } .g { fill: #999; }
    }
  </style>
  <defs>
    <marker id="a4" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/>
    </marker>
  </defs>

  <text class="g" x="26" y="40">already there</text>
  <text class="s" x="206" y="40">built inside the block</text>
  <text class="g" x="452" y="40">never touched</text>

  <rect class="arena" x="20" y="52" width="600" height="48" rx="3"/>
  <rect class="b" x="26" y="56" width="80" height="40" rx="2"/>
  <rect class="b" x="112" y="56" width="80" height="40" rx="2"/>
  <rect class="r" x="206" y="56" width="68" height="40" rx="2"/>
  <text class="t" x="216" y="80">String</text>
  <rect class="r" x="280" y="56" width="68" height="40" rx="2"/>
  <text class="t" x="290" y="80">String</text>
  <rect class="r" x="354" y="56" width="72" height="40" rx="2"/>
  <text class="t" x="368" y="80">[Line]</text>

  <line class="tick" x1="200" y1="44" x2="200" y2="112"/>
  <text class="s" x="176" y="128">mark</text>
  <line class="tick" x1="432" y1="44" x2="432" y2="112"/>
  <text class="t" x="416" y="128">top</text>

  <path class="a" d="M432 140 C 432 176, 200 176, 200 142"/>
  <text class="s" x="242" y="198">the block ends:  top = mark</text>
  <text class="g" x="242" y="214">one assignment, whatever is above it</text>
</svg>

`top` is a **bump pointer** — allocating means moving it right, which is one add. Opening a region
writes down where `top` stood; closing it puts `top` back. **Release is O(1) however many values are
above the mark**: no traversal, no finalizers, nothing to schedule, and no pause anyone will ever
measure.

## You do not have to write any of it down

```burxt
let message: String = "line " + to_string(42);
print(message);
```

That is a complete program. A program has a tray from the moment it starts, and until v0.0.146 you
had to say so — building anything outside a `region` was a compile error, so every file opened with
a wrapper it never mentioned again. That is gone.
([Why, and what it cost to find out](../../spec/M14-IMPLICIT-REGIONS.md).)

## `region` — a second tray, for work you want off your hands early

```burxt
region r {
    let message: String = "line " + to_string(42);
    print(message);
}
// everything built inside is gone here
```

Since you no longer *have* to, the question is when you should. Here is the number that answers it —
a loop building 100,000 Strings, peak memory:

<div class="tablewrap" markdown="1">

| | |
|---|---|
| no region | **5,280 KB** |
| `region each { ... }` around the loop body | **1,408 KB** |

</div>

Nothing is released until the program exits, so memory grows in a **straight line**. For three
Strings that costs nothing at all. For a loop over a million rows it costs everything: the arena is
a 1 GB reservation, and a server loop will get there.

So the rule of thumb is one sentence — **a region per unit of work whose results you do not keep:**

```burxt
function more_rows() -> Bool { return false; }
function next_row() -> Int { return 0; }
function handle(row: Int) -> Int { return row; }

while more_rows() {
    region row { let ignored: Int = handle(next_row()); }
}
```

That loop uses the same memory on its first row and its millionth.

## Building something for your caller

A function body has no tray of its own — which sounds like it would make a helper that formats a
message impossible to write. It does not: a function that builds a value builds it **in its
caller's region**.

```burxt
function describe(line: Int) -> String {
    return "line " + to_string(line) + ": unexpected byte";
}

region parse {
    print(describe(3));      // the bytes belong to `parse`
}
```

The value never outlives the region it was built in, so the rule holds by construction rather than
by anybody checking.

Until v0.0.142 `describe` had to be written `-> String allocates`, declaring that it built
something. Writing it still works and is still verified, and it is still **required** on
`external function`, where there is no body to look at. But it is no longer required of you, for a
reason worth knowing: the compiler always derived the answer anyway, and in
[`examples/pos/receipt.bx`](../../examples/pos/receipt.bx) the word landed on **three functions out
of three**. An annotation that appears on everything tells a reader nothing, and a reader who learns
to skip one annotation has learned to skip annotations.

That is the opposite call from the one made for [`touches`](06-effects.md), and deliberately so:
`allocates` carried no promise anyone needed, and `touches network` *is* the promise.

## What cannot leave

One rule, and it is the one from the top of this page: **a value built inside a `region` block
cannot leave it**, because that block releases at its closing brace.

```burxt
function bad() -> String {
    region r { return "a" + "b"; }
}
```

```
error: cannot return this String: it was built inside a `region` block, which releases at
       its closing brace, so its storage would not outlive the call. Move the allocation
       out of the `region` block, or return a scalar summary.
```

It applies **through a name** as well as directly, and that distinction is exactly the empty string
from the top of the page. The check used to ask whether the returned *expression* allocated — and a
name is not an expression that allocates, it is a name that happens to hold one:

```burxt
function bad(tag: Int) -> String {
    region inner {
        let s: String = "secret-" + to_string(tag);
        return s;
    }
}
```

Same words:

```
error: cannot return this String: it was built inside a `region` block, which releases at
       its closing brace, so its storage would not outlive the call. Move the allocation
       out of the `region` block, or return a scalar summary.
```

It also looks **inside aggregates**: a class holding a built String is itself built, and an array of
them likewise. That arm was missing for four versions and let a second use-after-free through, which
is why it walks fields and elements now. Both refusals with the compiler's exact words are in
[`examples/regions.bx`](../../examples/regions.bx).

## Two limits to know about

**Regions do not nest yet.**

```
error: `region b` cannot open inside `region a` — nested regions are not available yet.
       Close the outer one first, or use a single region for both.
```

**Running out is a named failure, not a crash.** The reservation is 1 GB per process, lazily mapped,
so the cost is virtual until used:

```
burxt runtime error: region memory exhausted — this build reserves 1 GB per process
```

An allocator that does not check its own limit does not fail — it corrupts, and then you are
debugging the wrong thing. That distinction cost a session in v0.0.73, and is why the check is in
both compilers.

## Next

[Contracts](05-contracts.md) — claims about a function that the compiler checks, and the reason
`burxt review` has anything to read.

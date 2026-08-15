---
title: Memory
description: A region is a cafeteria tray. You pile work onto it and tip the whole thing at the door — no collector, no reference counts, no lifetimes.
---

> **This documents Burxt 1.0.0, and it is frozen.** It will not change again — that is what makes
> it useful to someone still on 1.0. The current documentation is at
> [the top of the site]({{ site.baseurl }}/).


# 4. Memory

## What this is for
{: #what-this-is-for}

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
is a bug that ships.

## Think of a cafeteria tray
{: #think-of-a-cafeteria-tray}

You take a tray. You pile things on it — a bowl, a cup, a plate — and you do not think about any of
them individually, because you are not going to. When you are finished you carry the whole tray to
the door and tip it in one motion.

Nobody walks around the room deciding which fork is still needed. That is a garbage collector, and it
is why a cafeteria does not have one.

<figure>
<svg viewBox="0 0 680 316" role="img" aria-label="A region is a cafeteria tray: everything a batch of work builds goes on it, and the whole tray is tipped in one motion when the batch is done. A value built on the tray cannot leave with you." style="max-width:100%;height:auto;">
  <style>
    .tray { fill: #f5f5f7; stroke: #1d1d1f; stroke-width: 2; }
    .dish { fill: #ffffff; stroke: #1d1d1f; stroke-width: 1.6; }
    .bin  { fill: #ffffff; stroke: #1d1d1f; stroke-width: 2; }
    .lid  { fill: none; stroke: #1d1d1f; stroke-width: 2; stroke-linecap: round; }
    .move { fill: none; stroke: #0071e3; stroke-width: 2; marker-end: url(#mb); }
    .stop { fill: none; stroke: #c8102e; stroke-width: 2; marker-end: url(#mr); }
    .no   { fill: none; stroke: #c8102e; stroke-width: 2; }
    .hair { fill: none; stroke: #d2d2d7; stroke-width: 1; }
    .h    { font: 600 13.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .t    { font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #1d1d1f; }
    .cap  { font: 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .red  { font: 600 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #c8102e; }
    .blue { font: 600 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #0071e3; }
  </style>
  <defs>
    <marker id="mb" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="#0071e3"/>
    </marker>
    <marker id="mr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="#c8102e"/>
    </marker>
  </defs>

  <text class="h" x="8" y="18">One tray holds everything this batch builds</text>

  <rect class="tray" x="20" y="36" width="256" height="106" rx="12"/>
  <rect class="hair" x="30" y="46" width="236" height="86" rx="8"/>

  <path class="dish" d="M56 82 h52 a26 26 0 0 1 -52 0 z"/>
  <path class="lid"  d="M52 82 h60"/>
  <text class="t" x="52" y="124">a String</text>

  <rect class="dish" x="142" y="64" width="34" height="38" rx="5"/>
  <path class="lid" d="M176 73 q11 9 0 18"/>
  <text class="t" x="132" y="124">to_string</text>

  <ellipse class="dish" cx="228" cy="84" rx="24" ry="11"/>
  <text class="t" x="210" y="124">[Line]</text>

  <text class="blue" x="290" y="82">done</text>
  <path class="move" d="M288 92 h48"/>

  <path class="bin" d="M356 58 h84 l-9 84 a10 10 0 0 1 -10 9 h-46 a10 10 0 0 1 -10 -9 z"/>
  <path class="lid" d="M349 52 h98"/>
  <path class="lid" d="M382 46 h32"/>
  <text class="cap"  x="346" y="170">one motion</text>
  <text class="blue" x="346" y="187">release is O(1)</text>

  <text class="cap" x="480" y="72">No collector:</text>
  <text class="cap" x="480" y="89">nothing needs</text>
  <text class="cap" x="480" y="106">finding.</text>
  <text class="cap" x="480" y="132">No counts:</text>
  <text class="cap" x="480" y="149">nothing is</text>
  <text class="cap" x="480" y="166">counted.</text>

  <line class="hair" x1="8" y1="208" x2="672" y2="208"/>

  <text class="h" x="8" y="234">You cannot take the fork home</text>

  <rect class="tray" x="20" y="248" width="150" height="54" rx="10"/>
  <path class="dish" d="M50 260 v32 M60 260 v32 M70 260 v32"/>
  <path class="dish" d="M50 260 h20 v12 a10 10 0 0 1 -20 0 z"/>
  <text class="t" x="92" y="270">a String</text>
  <text class="t" x="92" y="287">built here</text>

  <path class="stop" d="M186 275 h44"/>
  <g class="no">
    <circle cx="256" cy="275" r="14"/>
    <line x1="246" y1="265" x2="266" y2="285"/>
  </g>

  <text class="red" x="284" y="270">A value built on the tray</text>
  <text class="red" x="284" y="287">cannot outlive it.</text>
</svg>
<figcaption>The tray <em>is</em> the lifetime, so there is nothing else to prove — and the compiler says so
before the program runs rather than after it has returned believable bytes.</figcaption>
</figure>

No collector, because nothing needs finding. No reference counts, because nothing is counted. No
lifetimes to prove, because the tray is the lifetime.

## A step closer
{: #a-step-closer}

The tray is an arena, and it has exactly two moving parts: a pointer to the top, and a mark.

<svg viewBox="0 0 640 222" role="img" aria-label="A region is a bump pointer and a mark; closing it puts the pointer back" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .arena { fill: none; stroke: #1d1d1f; stroke-width: 1.5; }
    .b { fill: #fff; stroke: #1d1d1f; stroke-width: 1.2; }
    .r { fill: #fff; stroke: #c8102e; stroke-width: 1.5; stroke-dasharray: 4 3; }
    .tick { stroke: #1d1d1f; stroke-width: 2.5; }
    .t { font: 11px ui-monospace, monospace; fill: #1d1d1f; }
    .g { font: 11px ui-monospace, monospace; fill: #3a3a3e; }
    .s { font: 12px ui-monospace, monospace; fill: #c8102e; }
    .a { stroke: #c8102e; stroke-width: 1.8; fill: none; marker-end: url(#a4); }
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

## In code
{: #in-code}

You do not have to write any of it down. This is a complete program:

```burxt
let message: String = "line " + to_string(42);
print(message);
```

A program has a tray from the moment it starts. Until v0.0.146 you had to say so — building anything
outside a `region` was a compile error, so every file opened with a wrapper it never mentioned again.
That is gone.
([Why, and what it cost to find out](https://github.com/andrecorugda/burxt/blob/main/spec/1.0/M14-IMPLICIT-REGIONS.md).)

**Every block already gives the tray back.** When a block ends, anything built inside it that
nothing outside can still reach is released — one assignment to a pointer, exactly as the picture
above shows. You do not write anything to get that:

```burxt
let mutable width: Int = 0;
let mutable i: Int = 0;
while i < 100000 {
    let label: String = "row {i}";      // built here
    width = len(label);                 // only the LENGTH escapes
    i += 1;
}
```

A hundred thousand Strings are built and a hundred thousand are released, because the loop body can
prove none of them leaves. Measured on that exact program: **1,408 KB, flat**. Before per-block
release it was **5,280 KB and climbing** — the memory grew with the loop, and a long-running server
would eventually have hit the wall.

**The proof is what makes it safe, and it is the same rule as everywhere else in this page**: a value
may not outlive the block it was built in. If something *does* escape, the block simply keeps its
memory — the behaviour you had before — rather than freeing something you can still reach. Change one
line of that loop:

```burxt
    last = label;                        // now the String escapes
```

and the loop is back to 5,280 KB, on purpose. **A block that cannot prove it is safe to release does
not release.** The failure direction is memory, never a dangling pointer.

**`region` is still here, and it is now for the case the compiler cannot prove.** It is a promise you
make instead of one the compiler derives:

```burxt
region r {
    let message: String = "line " + to_string(42);
    print(message);
}
// everything built inside is gone here
```

Reach for it when a block holds something the analysis has to be conservative about, or when you want
a scope narrower than the block structure gives you. Most programs no longer need it at all —
[`examples/pos/`](https://github.com/andrecorugda/burxt/blob/main/examples/pos/) has none.

**A function that builds a value builds it in its caller's region.** A body has no tray of its own,
which sounds like it would make a helper that formats a message impossible to write. It does not:

```burxt
function describe(line: Int) -> String {
    return "line " + to_string(line) + ": unexpected byte";
}

region parse {
    print(describe(3));      // the bytes belong to `parse`
}
```

The value never outlives the region it was built in, so the rule holds by construction rather than by
anybody checking.

Until v0.0.142 `describe` had to be written `-> String allocates`, declaring that it built something.
Writing it still works and is still verified, and it is still **required** on `external function`,
where there is no body to look at. But it is no longer required of you, for a reason worth knowing:
the compiler always derived the answer anyway, and in
[`examples/pos/receipt.bx`](https://github.com/andrecorugda/burxt/blob/main/examples/pos/receipt.bx)
the word landed on **three functions out of three**. An annotation that appears on everything tells a
reader nothing, and a reader who learns to skip one annotation has learned to skip annotations.

That is the opposite call from the one made for [`touches`](06-effects.md), and deliberately so:
`allocates` carried no promise anyone needed, and `touches network` *is* the promise.

### `allocates nothing` — the claim that runs the other way
{: #allocates-nothing}

There is a marker worth writing, and it is the mirror image of the one that stopped being worth it:

```burxt
function widest(xs: [Int]) -> Int allocates nothing
    requires len(xs) > 0
{
    let mutable best: Int = xs[0];
    let mutable i: Int = 1;
    while i < len(xs) {
        if xs[i] > best { best = xs[i]; }
        i += 1;
    }
    return best;
}
```

The difference is who is being trusted. `allocates` was you telling the compiler something it worked out
anyway. **`allocates nothing` is you asking the compiler to hold you to it** — and that is the useful
direction, because a function that quietly *starts* allocating is how a constant-memory loop stops being
one, and nothing else in the language would notice.

It is **transitive**, because the inference is. A function that calls something which allocates does
allocate, and a claim that stopped at the first call would pass exactly when the allocation was one
level away — which is where it usually is:

```
error: `function outer` claims `allocates nothing`, and it does allocate — `inner(...)` builds its
       answer in the caller's region. Either drop the claim, or move the building into a function
       that does not make it.
```

Three ways to break it, and the message names the cause in each: directly, through a call, and through
a `dynamic`. The last is the one hardest to spot by reading — the body says `thing.name()` and nothing
about it looks like an allocation — and there the claim has to hold for **every** implementation, so one
that allocates is enough.

**Where it earns its place:** the body of a hot loop, a comparison function, anything you intend to stay
free of the region. Where it does not: everywhere. It is a claim about a promise you are making, so it
belongs where the promise matters — the same rule `touches` follows.

### Asking, instead of annotating
{: #explain-memory}

The honest cost of inferring `allocates` is that the memory story left the source. The answer is not to
put the annotation back — it is to make the fact **queryable**, because it is wanted occasionally rather
than always:

```
$ burxt explain memory examples/pos/receipt.bx
   33  Line.subtotal()      nothing
   95  line_tax()           nothing
  122  money_column()       `to_string(...)` builds a String
                            joining two Strings builds a new one
  133  line_text()          joining two Strings builds a new one
```

That is strictly more than `allocates` ever said: **whether and what**, not just whether. It answers
from the same inference the compiler uses, so it cannot disagree with `allocates nothing` — a test holds
the two against each other function by function.

What it does not yet say is **where** the value lands, and which block releases it. That is per-block
release, which is not built, and the command says so at the bottom of its own output rather than leaving
you to assume the table is complete.

### What cannot leave
{: #what-cannot-leave}

One rule, and it is the one from the top of this page: **a value built inside a `region` block cannot
leave it**, because that block releases at its closing brace.

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
name is not an expression that allocates, it is a name that happens to hold one.

It also looks **inside aggregates**: a class holding a built String is itself built, and an array of
them likewise. That arm was missing for four versions and let a second use-after-free through, which
is why it walks fields and elements now. Both refusals with the compiler's exact words are in
[`examples/regions.bx`](https://github.com/andrecorugda/burxt/blob/main/examples/regions.bx).

## Why it is built this way
{: #why-it-is-built-this-way}

**It is the only memory model that needs no annotation and has no pause.** Those are usually a
trade-off: a collector costs you the pause, and proving lifetimes costs you the annotations. A region
costs neither, because it answers a smaller question. Not *"when is this value dead?"* — which needs
tracking — but *"when is this batch of work over?"*, which you already know, because you wrote the
loop.

**Release is O(1), and that is a guarantee rather than a measurement.** One assignment puts `top`
back. It does not matter whether the block built three values or three million.

**A reviewer needs to know nothing.** This is the part that matters for the language's purpose. There
is no annotation to read, no lifetime to follow across a signature, and nothing an agent can get
subtly wrong — because the one rule it could break is a compile error that names the line.

**Failure is named, not silent.** The reservation is 1 GB per process and the allocator checks its own
limit. An allocator that does not check does not fail — it corrupts, and then you are debugging the
wrong thing. That distinction cost a session in v0.0.73, and is why the check is in both compilers.

## What it costs
{: #what-it-costs}

**Nothing is released until the program exits unless you ask.** Memory grows in a **straight line**.
For three Strings that costs nothing at all. For a loop over a million rows it costs everything.

Measured, on a loop building 100,000 Strings — peak memory:

<div class="tablewrap" markdown="1">

| | |
|---|---|
| no region | **5,280 KB** |
| `region each { ... }` around the loop body | **1,408 KB** |

</div>

**Regions do not nest yet.**

```
error: `region b` cannot open inside `region a` — nested regions are not available yet.
       Close the outer one first, or use a single region for both.
```

**A value cannot escape, which occasionally means restructuring.** If a helper wants to return
something it built inside a `region`, the region has to move out or the helper has to return a scalar
summary. That is a real constraint and not a large one — but it is the one thing about this model you
will meet.

**Running out is a failure you can hit.**

```
burxt runtime error: region memory exhausted — this build reserves 1 GB per process
```

## When you reach for it
{: #when-you-reach-for-it}

The rule of thumb is one sentence: **a region per unit of work whose results you do not keep.**

<div class="tablewrap" markdown="1">

| You are writing | Do this |
|---|---|
| a short program, a script, a few dozen values | nothing. A program has a tray already |
| a loop over rows, requests, files, lines | `region` around the **body** — this is the one that matters |
| a long-running server | `region request { ... }` around each request. That is the whole of keeping its memory flat |
| a helper that formats or joins something | nothing. It builds in its caller's region |
| an `external function` that allocates | `allocates` — required there, because there is no body to look at |
| a helper you need to STAY free of the region | `allocates nothing` — a claim the compiler holds you to, transitively |
| something you need to return from inside a region | move the region out, or return a scalar |

</div>

## Examples
{: #examples}

**A loop whose memory does not grow.** The region closes on every pass, so this program uses the same
memory on its first row and its millionth:

```burxt
function describe(row: Int) -> String {
    return "row " + to_string(row) + " handled";
}

let mutable i: Int = 1;
while i <= 3 {
    region each {
        print(describe(i));
    }
    i = i + 1;
}
print("peak memory did not grow with the loop");
```

```
row 1 handled
row 2 handled
row 3 handled
peak memory did not grow with the loop
```

**And the refusal that makes it safe.** This is the use-after-free from the top of the page, caught:

```burxt
function leaked(tag: Int) -> String {
    region inner {
        let s: String = "secret-" + to_string(tag);
        return s;
    }
}
```

```
error: cannot return this String: it was built inside a `region` block, which releases at its closing brace, so its storage would not outlive the call. Move the allocation out of the `region` block, or return a scalar summary.
 --> leaked.bx:4:9
  |
4 |         return s;
  |         ^^^^^^^^^
```

Note where the caret is: on `return s`, not on the line that built the String. The value escapes
**through a name**, and for four versions the check missed exactly that — it asked whether the
returned *expression* allocated, and a name is not an expression that allocates.

## Next
{: #next}

[Contracts](05-contracts.md) — claims about a function that the compiler checks, and the reason
`burxt review` has anything to read.

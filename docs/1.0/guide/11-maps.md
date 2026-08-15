---
title: Maps and strings
description: A map is a cloakroom — you get a ticket back, and the coats hang in the order they arrived, always.
---

> **This documents Burxt 1.0.0, and it is frozen.** It will not change again — that is what makes
> it useful to someone still on 1.0. The current documentation is at
> [the top of the site]({{ site.baseurl }}/).


# 11. Maps and strings

## What this is for
{: #what-this-is-for}

You print a map for a log line. The test asserts on the output. It passes on your machine, passes in
review, and fails in CI — not always, just often enough that somebody adds a retry.

The cause is that in most languages iteration order is *whatever the hash function did*. Go
**randomises** it deliberately, so nobody can depend on it. Rust **randomises its hash seed per
process**, so the order differs between runs. Both of those are admissions: hash order leaked into
programs and then broke them, and the only fix left was to make the leak loud.

Burxt takes the other end. **Iteration order is insertion order.** Not *unspecified*, not *arbitrary*,
not *do not rely on it* — defined.

That is not taste. The thesis of this language is that the same inputs produce the same bytes — it is
what the byte-identical self-hosting fixpoint exists to prove — and a container whose iteration order
depends on a hash function's internals is a determinism hazard sitting in the middle of it. Printing a
map, serialising one, or hashing a structure containing one would all become run-dependent, and none
of those should be.

## Think of a cloakroom
{: #think-of-a-cloakroom}

You hand over a coat and get a numbered ticket. Later you hand back the ticket and get the coat.

And the coats hang on the rail **in the order they arrived**. Not in an order the cloakroom attendant
finds convenient, and certainly not in a different order each evening — because when you walk past the
rail looking for yours, an order you can predict is the entire point.

<figure>
<svg viewBox="0 0 680 258" role="img" aria-label="A map as a cloakroom: a key is a ticket, and iteration walks the rail in the order the coats arrived — never in hash order, and a replaced coat keeps its original place" style="max-width:100%;height:auto;">
  <style>
    .rail  { fill: none; stroke: #1d1d1f; stroke-width: 3; stroke-linecap: round; }
    .hook  { fill: none; stroke: #1d1d1f; stroke-width: 1.4; }
    .coat  { fill: #ffffff; stroke: #1d1d1f; stroke-width: 1.6; }
    .cfill { fill: #0071e3; opacity: .08; }
    .swap  { fill: #0f6f3c; opacity: .12; }
    .tick  { fill: #ffffff; stroke: #0f6f3c; stroke-width: 1.6; }
    .hair  { fill: none; stroke: #d2d2d7; stroke-width: 1; }
    .shuf  { fill: none; stroke: #c8102e; stroke-width: 2; stroke-dasharray: 5 4; }
    .no    { fill: none; stroke: #c8102e; stroke-width: 2; }
    .h     { font: 600 13.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .t     { font: 11.5px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #1d1d1f; }
    .n     { font: 600 11px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #0f6f3c; }
    .cap   { font: 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .red   { font: 600 11.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #c8102e; }
  </style>

  <text class="h" x="8" y="18">The rail, in arrival order</text>

  <path class="rail" d="M20 46 h420"/>
  <g>
    <path class="hook" d="M60 46 v12"/>
    <rect class="cfill" x="34" y="58" width="52" height="56" rx="6"/>
    <rect class="coat"  x="34" y="58" width="52" height="56" rx="6"/>
    <text class="t" x="40" y="130">pear</text>
    <text class="n" x="40" y="146">1st</text>
  </g>
  <g>
    <path class="hook" d="M160 46 v12"/>
    <rect class="swap" x="134" y="58" width="52" height="56" rx="6"/>
    <rect class="coat" x="134" y="58" width="52" height="56" rx="6"/>
    <text class="t" x="136" y="130">apple</text>
    <text class="n" x="136" y="146">2nd</text>
  </g>
  <g>
    <path class="hook" d="M260 46 v12"/>
    <rect class="cfill" x="234" y="58" width="52" height="56" rx="6"/>
    <rect class="coat"  x="234" y="58" width="52" height="56" rx="6"/>
    <text class="t" x="242" y="130">fig</text>
    <text class="n" x="242" y="146">3rd</text>
  </g>

  <rect class="tick" x="330" y="66" width="96" height="40" rx="6"/>
  <text class="t" x="340" y="84">ticket:</text>
  <text class="t" x="340" y="100">"apple"</text>

  <text class="cap" x="20" y="176">A coat handed in twice keeps its</text>
  <text class="cap" x="20" y="194">original hook — <tspan font-family="ui-monospace, monospace">apple</tspan> is still 2nd.</text>

  <line class="hair" x1="470" y1="8" x2="470" y2="230"/>

  <text class="h" x="500" y="18">Not this</text>
  <path class="rail" d="M500 46 h160"/>
  <path class="hook" d="M524 46 v12"/>
  <rect class="coat" x="504" y="58" width="40" height="46" rx="5"/>
  <path class="hook" d="M584 46 v12"/>
  <rect class="coat" x="564" y="58" width="40" height="46" rx="5"/>
  <path class="hook" d="M636 46 v12"/>
  <rect class="coat" x="616" y="58" width="40" height="46" rx="5"/>
  <path class="shuf" d="M508 122 q76 30 148 0"/>
  <g class="no">
    <circle cx="582" cy="150" r="13"/>
    <line x1="573" y1="141" x2="591" y2="159"/>
  </g>
  <text class="red" x="486" y="188">hash order, reshuffled</text>
  <text class="red" x="486" y="206">whenever the table grows</text>

  <text class="cap" x="8" y="250">Go randomises iteration deliberately; Rust randomises its hash seed per process.</text>
</svg>
<figcaption>Iteration is <strong>insertion order, always</strong>. Not "unspecified", not "arbitrary" — a
language whose thesis is reproducibility should not ship a container whose order depends on a hash
function's internals. That Go and Rust both randomise theirs deliberately is an admission that hash order
leaked into programs and then broke them.</figcaption>
</figure>

## A step closer
{: #a-step-closer}

The rail is two arrays rather than buckets of lists.

`entries` holds the coats in arrival order, and that is what iteration walks. `slots` is the ticket
table — open addressed, linear probing, holding an index into `entries` **plus one**, so that a zero can
mean *empty* without a sentinel constant. A removed entry keeps its place as a tombstone so everything
after it holds its position, and `count()` answers the ones that are still coats.

That shape is right here for a reason particular to Burxt: **there is no per-entry allocation at all.**
Every allocation lands in a [region](04-memory.md), and a region is a bump pointer — so a container that
allocated once per insertion would make the region grow for no reason.

## In code
{: #in-code}

### Using one

```burxt
use "lib/map.bx";

let mutable counts: Map<String, Int> = map_new();
let added: Int = counts.set("apples", 3);
let again: Int = counts.set("pears", 7);

print(counts.count());              // 2
print(counts.get("apples", 0));     // 3
print(counts.get("plums", 0));      // 0 — the fallback
print(counts.has("pears"));         // true

let keys: [String] = counts.keys();     // in the order they went in
for k in keys {
    print(k);
}

let removed: Bool = counts.remove("apples");   // true; false if it was not there
print(removed);
```

`lib/map.bx` is a **library file**, like `lib/option.bx` and `lib/result.bx`. Ordinary Burxt written
with the [generics](09-generics.md), and the only compiler support it needs is one builtin, `hash`. If
a map had needed a keyword, those generics would not be real.

### Reading: `get` or `find`

Two ways, and the difference is whether a default is the right answer.

```burxt
use "lib/map.bx";

let mutable counts: Map<String, Int> = map_new();
let added: Int = counts.set("pears", 7);

print(counts.get("plums", 0));          // the value, or the fallback you gave

match counts.find("pears") {            // the value, or None
    None => { print("no pears"); }
    Some(n) => { print(n); }
}
```

`get` is for when a default is genuinely right — a missing count is zero. `find` answers an
`Option<V>`, [the one type](10-absence-and-failure.md) that cannot be read without saying what happens
when there is nothing there.

`find` could not be written until v0.0.118, and the reason shows how these pieces lean on each other: a
variant payload had to be a scalar, so `Option<Point>` was refused, so an `Option<V>` return would have
restricted map **values** to scalars. `get` with a fallback was the honest answer while that was true.
When the payload rule lifted, `find` was three lines.

### Keys are `Equatable`

`Int`, `Bool`, `String`, `Decimal` — exactly the types `==` works on, which is the
[bound](09-generics.md#bounds) the generics page already describes. A key needs equality and a hash,
and the set of types that have equality is the set that can have one. So there is no `Hashable` bound:
it would name the same four types twice.

```burxt
use "lib/map.bx";

let mutable by_number: Map<Int, String> = map_new();
let one: Int = by_number.set(1, "one");
print(by_number.get(1, "?"));           // one
```

A **class** as a key is refused. It would need structural hashing, which needs a per-type walk, which
needs either a derive mechanism or an interface with a method — both larger than this container. For a
compound key, build a `String` from the parts.

Values have no such restriction:

```burxt
use "lib/map.bx";

class Point { x: Int, y: Int }

let mutable places: Map<String, Point> = map_new();
let put: Int = places.set("origin", Point { x: 1, y: 2 });
let here: Point = places.get("origin", Point { x: 0, y: 0 });
print(here.y);
```

### Everything is a method except `map_new`

```burxt
function map_new<K: Equatable, V>() -> Map<K, V>                     // an empty map
function (mutable self: Map<K, V>) set(key: K, value: V) -> Int       // 1 if new, 0 if it replaced
function (self: Map<K, V>) get(key: K, fallback: V) -> V
function (self: Map<K, V>) find(key: K) -> Option<V>
function (self: Map<K, V>) has(key: K) -> Bool
function (mutable self: Map<K, V>) remove(key: K) -> Bool
function (self: Map<K, V>) count() -> Int
function (self: Map<K, V>) keys() -> [K]
```

Methods rather than free functions, and that was **forced rather than chosen**: Burxt has no writable
parameters, so a container that changes has to change through `mutable self`. The API is better for it
— `counts.set("k", 1)` reads better than `map_set(counts, "k", 1)` — which is the usual way a real
constraint turns out to have been pointing at the nicer design all along.

`set` answers `1` when the key is new and `0` when it replaced a value, so counting distinct keys needs
no second lookup.

### Strings

A `String` is **bytes**. Not a rope, not a UTF-16 array, not an object with a hidden encoding field.

```burxt
print(len("hello"));                 // 5 — bytes, not characters
print(byte_at("hello", 0));          // 104
print(substring("hello", 1, 3));     // ell — a start and a LENGTH
print("total: " + to_string(3));     // joining builds a new String
```

Interpolation is a join written differently — `"total: {amount}"` — and `+` on two Strings builds a new
one, which means it allocates, which means [Memory](04-memory.md) applies.

`lib/string.bx` is where the rest lives, and it is ordinary Burxt too:

<div class="tablewrap" markdown="1">

| | |
|---|---|
| `string_find(text, needle)` | the byte offset, or `-1` |
| `string_contains` / `string_starts_with` / `string_ends_with` | `Bool` |
| `string_trim(text)` | leading and trailing whitespace removed |
| `string_split(text, separator)` | the separator is a **String**, so `", "` and `"\r\n"` both work |
| `string_lines(text)` | split on newlines |
| `string_join(pieces, separator)` | the separator here *is* a String |
| `string_to_int(text, fallback)` | the number, or the fallback you named |
| `string_parse_int(text)` | `Option<Int>` — for when garbage is not zero |
| `string_repeat(text, times)` | |

</div>

Those last two are a pair on purpose. `string_to_int` used to be the only one and it answered `0` for
garbage — which is the silent-wrong-answer shape this whole language is against, and it took a real
bug to notice. Now the fallback is either named by the caller or handed back as an `Option`.

One honest gap: there is no case conversion yet. The split separator was a single byte until
v0.0.189 — so `", "` and `"\r\n"` could not be split on at all — and it is a String now.

## Why it is built this way
{: #why-it-is-built-this-way}

**Because a hash order that leaks into a program is a bug you find later.** Go randomises map iteration
deliberately and Rust randomises its hash seed per process, and both are admissions that programs came to
depend on an order nobody promised. A language whose whole argument is that a wrong answer must not be
plausible cannot ship a container whose output order changes between runs.

**Because it needs no keyword.** `Map<K, V>` is one file of ordinary Burxt, and the only compiler support
it asks for is `hash(x)`. If a map had needed a keyword, the [generics](09-generics.md) were not real.

### The shape, and why it is that shape

There is **no per-entry allocation**: a map is the two arrays in the diagram and nothing else. That
matters more here than in most languages, because every allocation lands in a region and a region is a
[bump pointer](04-memory.md) — a container that allocated once per insertion would make the region grow
for nothing.

Growing the table re-places the cards and drops the tombstones. It never reorders the pegs.

## What it costs
{: #what-it-costs}

**A tombstone stays.** Removing an entry keeps its place so everything after it holds its insertion
position. `count()` answers the live entries; the storage does not shrink until the region goes.

**Keys are `Equatable`** — `Int`, `Bool`, `String`, `Decimal`. Not a class of yours, because `==` does not
work on one.

**A String is bytes.** `len` counts bytes and `byte_at` reads one, so anything beyond ASCII is a
byte-by-byte question you have to answer yourself. There is no `.chars()` yet.

### What is deliberately absent

**No `unwrap`.** Same reason as [`lib/option.bx`](10-absence-and-failure.md): it is a decision
disguised as a convenience.

**No `map`, `filter` or `each`.** They need a function as a value, and a closure needs an owner for its
captured state — a memory question, not a syntax one. Iterate `keys()` with a `for` loop, which cannot
capture anything by accident.

**No HashDoS protection.** `hash` is deterministic and unseeded, because a seeded hash cannot iterate
in a defined order, and that is the whole point of the container. A caller feeding attacker-chosen keys
can force collisions and turn O(1) into O(n). If you need that guarded the answer will be a second
constructor — `map_seeded(seed)` — so the program that needs it *says* it needs it, rather than every
program paying for it. A security property should be visible in the code that has it.

## When you reach for it
{: #when-you-reach-for-it}

<div class="tablewrap" markdown="1">

| You want | Write |
|---|---|
| a fresh map | `let mutable m: Map<String, Int> = map_new();` — the type comes from the annotation |
| to insert or overwrite | `m.set(key, value)` — answers 1 if the key was new |
| a value, with a default | `m.get(key, fallback)` |
| a value, and to know whether it was there | `m.find(key)` — answers `Option<V>` |
| just to ask | `m.has(key)` |
| to walk everything, in order | `let names = m.keys();` then `for k in names` |
| how many live entries | `m.count()` |

</div>

`for` iterates a **named** array, so bind `keys()` first: a method call in the `for` header would be
recomputed on every pass, and the compiler refuses it rather than doing that quietly.

## Examples
{: #examples}

**Insertion order, including an overwrite.** `apple` is set twice and keeps its original place:

```burxt
use "lib/map.bx";

let mutable counts: Map<String, Int> = map_new();
let a: Int = counts.set("pear", 2);
let b: Int = counts.set("apple", 5);
let c: Int = counts.set("fig", 1);
let d: Int = counts.set("apple", 6);

print(counts.count());
let names: [String] = counts.keys();
for key in names {
    print(key + " " + to_string(counts.get(key, 0)));
}
```

```
3
pear 2
apple 6
fig 1
```

Three live entries, not four. `apple` holds its **second** position with its **new** value — which is the
behaviour you would have assumed, and the one most languages do not give you.

**And the refusal that keeps a loop honest**, if you skip the binding:

```burxt
use "lib/map.bx";

let mutable counts: Map<String, Int> = map_new();
let a: Int = counts.set("pear", 2);
for key in counts.keys() {
    print(key);
}
```

```
error: `for` iterates a named array, and this is a method call: its result would be recomputed on every pass. Bind it first — `let items = ...;` — and iterate that.
 --> counts.bx:5:26
  |
5 | for key in counts.keys() {
  |                          ^
```

## Next
{: #next}

[Tools and agents](12-tools-and-agents.md) — `burxt mcp-schema`, which derives an agent's tool schema
from the preconditions so the two cannot drift, and `burxt review`, which answers what a change did to
what a program promises.

Or the [reference]({{ site.baseurl }}/reference/) for every keyword, builtin, command and
standard-library function — generated by reading the compiler, with a search box.

The design record carries the reasoning behind every refusal above:
[`spec/1.0/M11-MAPS.md`](https://github.com/andrecorugda/burxt/blob/main/spec/1.0/M11-MAPS.md).

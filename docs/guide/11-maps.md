---
title: Maps and strings
---

# 11. Maps and strings

## The problem, as it actually arrives

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

Coats hang on pegs **in the order they arrived**. That is the entries array, and it is what iteration
walks.

The attendant also keeps a box of index cards so they do not have to check every peg to find your
coat. That is the hash table, and the cards are in whatever order hashing put them — which is fine,
because nobody ever reads the cards in order.

<svg viewBox="0 0 640 244" role="img" aria-label="A map is entries in insertion order plus a hash table of positions into them" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .b { fill: #fff; stroke: #111; stroke-width: 1.5; }
    .p { fill: none; stroke: #b00; stroke-width: 1.5; stroke-dasharray: 4 3; }
    .t { font: 11px ui-monospace, monospace; fill: #111; }
    .g { font: 11px ui-monospace, monospace; fill: #888; }
    .s { font: 11px ui-monospace, monospace; fill: #b00; }
    .a { stroke: #888; stroke-width: 1.2; fill: none; marker-end: url(#a11); }
    @media (prefers-color-scheme: dark) {
      .b { fill: #1b1b1b; stroke: #ddd; } .t { fill: #eee; } .s { fill: #ff8080; }
      .p { stroke: #ff8080; } .a { stroke: #999; } .g { fill: #999; }
    }
  </style>
  <defs>
    <marker id="a11" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/>
    </marker>
  </defs>

  <text class="g" x="20" y="20">the pegs — the order the coats arrived, and the order you iterate</text>
  <text class="g" x="104" y="38">0</text>
  <text class="g" x="222" y="38">1</text>
  <text class="g" x="340" y="38">2</text>
  <text class="g" x="458" y="38">3</text>
  <rect class="b" x="60" y="44" width="110" height="44" rx="4"/>
  <text class="t" x="70" y="62">"apples"</text>
  <text class="t" x="70" y="78">3</text>
  <rect class="b" x="178" y="44" width="110" height="44" rx="4"/>
  <text class="t" x="188" y="62">"pears"</text>
  <text class="t" x="188" y="78">7</text>
  <rect class="p" x="296" y="44" width="110" height="44" rx="4"/>
  <text class="s" x="306" y="70">removed</text>
  <rect class="b" x="414" y="44" width="110" height="44" rx="4"/>
  <text class="t" x="424" y="62">"plums"</text>
  <text class="t" x="424" y="78">1</text>

  <text class="g" x="20" y="146">the index cards — hash to a position, in no order at all</text>
  <rect class="b" x="60" y="160" width="48" height="36" rx="3"/>
  <text class="g" x="80" y="183">–</text>
  <rect class="b" x="114" y="160" width="48" height="36" rx="3"/>
  <text class="t" x="134" y="183">0</text>
  <rect class="b" x="168" y="160" width="48" height="36" rx="3"/>
  <text class="g" x="188" y="183">–</text>
  <rect class="b" x="222" y="160" width="48" height="36" rx="3"/>
  <text class="t" x="242" y="183">3</text>
  <rect class="b" x="276" y="160" width="48" height="36" rx="3"/>
  <text class="g" x="296" y="183">–</text>
  <rect class="b" x="330" y="160" width="48" height="36" rx="3"/>
  <text class="g" x="350" y="183">–</text>
  <rect class="b" x="384" y="160" width="48" height="36" rx="3"/>
  <text class="t" x="404" y="183">1</text>
  <rect class="b" x="438" y="160" width="48" height="36" rx="3"/>
  <text class="g" x="458" y="183">–</text>

  <path class="a" d="M138 158 L118 92"/>
  <path class="a" d="M246 158 L462 92"/>
  <path class="a" d="M408 158 L236 92"/>

  <text class="g" x="20" y="230">so iteration is insertion order by construction, not by promise</text>
</svg>

Notice peg 2. A removed entry is **tombstoned in place**, so every entry after it keeps its position.
The cost is real and worth saying out loud: a tombstone is a hole rather than a slot to reuse freely,
and iteration walks the pegs rather than the cards. That is a constant factor. **Determinism is worth
a constant factor.**

## Using one

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

## Reading: `get` or `find`

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

## Keys are `Equatable`

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

## Everything is a method except `map_new`

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

## Strings

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
| `string_split(text, separator)` | the separator is **one byte** — `44` for a comma |
| `string_lines(text)` | split on newlines |
| `string_join(pieces, separator)` | the separator here *is* a String |
| `string_to_int(text, fallback)` | the number, or the fallback you named |
| `string_parse_int(text)` | `Option<Int>` — for when garbage is not zero |
| `string_repeat(text, times)` | |

</div>

Those last two are a pair on purpose. `string_to_int` used to be the only one and it answered `0` for
garbage — which is the silent-wrong-answer shape this whole language is against, and it took a real
bug to notice. Now the fallback is either named by the caller or handed back as an `Option`.

Two honest gaps: **the split separator is a single byte**, so `", "` and `"\r\n"` cannot be split on
yet, and there is no case conversion. Both are on the list.

## What is deliberately absent

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

## One thing about the shape

There is **no per-entry allocation**: a map is the two arrays in the diagram and nothing else. That
matters more here than in most languages, because every allocation lands in a region and a region is a
[bump pointer](04-memory.md) — a container that allocated once per insertion would make the region grow
for nothing.

Growing the table re-places the cards and drops the tombstones. It never reorders the pegs.

## Next

[Reference](reference.md) — every keyword, builtin, operator and error, in tables.

Or the design record, which carries the reasoning behind every refusal above:
[`spec/M11-MAPS.md`](../../spec/M11-MAPS.md).

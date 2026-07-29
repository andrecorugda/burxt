# 10. Maps

A key-value table, in **insertion order**.

```burxt
use "lib/map.bx";

region r {
    let mutable counts: Map<String, Int> = map_new();
    let added: Int = counts.set("apples", 3);
    let again: Int = counts.set("pears", 7);

    print(counts.count());              // 2
    print(counts.get("apples", 0));     // 3
    print(counts.get("plums", 0));      // 0 — the fallback
    print(counts.has("pears"));         // true
}
```

`lib/map.bx` is a **library file**, like `lib/option.bx` and `lib/result.bx`. It is ordinary Burxt
written with the generics from page 8, and the only compiler support it needs is one builtin,
`hash`. If a map had needed a keyword, the generics would not be real.

## Iteration order is insertion order. Always.

Not "unspecified". Not "arbitrary". Not "do not rely on it". **Defined.**

This is the one decision here worth arguing about, and the argument is not taste. Two major
languages shipped hash-ordered iteration and both had to walk it back:

- **Go randomises iteration deliberately**, so that nobody can depend on the order.
- **Rust randomises its hash seed per process**, so the order differs between runs.

Both are admissions that hash order leaked into programs that then broke. Burxt's thesis is
exactness and reproducibility — the same inputs producing the same bytes, which is what the
byte-identical self-hosting fixpoint exists to prove. A container whose iteration order depends on
a hash function's internals is a determinism hazard sitting in the middle of that. Printing a map,
serialising one, or hashing a structure containing one would all become run-dependent, and none of
those should be.

```burxt
let keys: [String] = counts.keys();     // in the order they went in
for k in keys {
    print(k);
}
```

Removal does not disturb it. A removed entry is **tombstoned in place**, so every entry after it
keeps its position:

```burxt
let removed: Bool = counts.remove("apples");   // true; false if it was not there
```

The cost is real and worth stating: a tombstone is a hole rather than a slot to reuse freely, and
iteration walks the entry array rather than the table. That is a constant factor. **Determinism is
worth a constant factor.**

## Keys are `Equatable`

`Int`, `Bool`, `String`, `Decimal` — exactly the types `==` works on, which is the bound page 8
already describes. A key needs equality and a hash, and the set of types that have equality is the
set that can have one. So there is no `Hashable` bound: it would name the same four types twice.

```burxt
let mutable by_number: Map<Int, String> = map_new();
let one: Int = by_number.set(1, "one");
print(by_number.get(1, "?"));           // one
```

A **record** as a key is refused. It would need structural hashing, which needs a per-type walk,
which needs either a derive mechanism or a trait with a method — both larger than this container.
For a compound key, build a `String` from the parts.

Values have no such restriction. A record value works:

```burxt
let mutable places: Map<String, Point> = map_new();
let put: Int = places.set("origin", Point { x: 1, y: 2 });
let here: Point = places.get("origin", Point { x: 0, y: 0 });
```

## Reading: `get` or `find`

Two ways, and the difference is whether a default is the right answer.

```burxt
print(counts.get("plums", 0));          // the value, or the fallback you gave

match counts.find("pears") {            // the value, or None
    None => { print("no pears"); }
    Some(n) => { print(n); }
}
```

`get` is for when a default is genuinely right — a missing count is zero. `find` answers an
`Option<V>`, which is the one type in this language that cannot be read without saying what happens
when there is nothing there (page 9).

`find` could not be written until v0.0.118, and the reason is worth knowing because it shows how
these pieces depend on each other: a variant payload had to be a scalar, so `Option<Point>` was
refused, so an `Option<V>` return would have restricted map **values** to scalars. `get` with a
fallback was the honest answer while that was true. When the payload rule lifted, `find` was three
lines.

## Everything is a method except `map_new`

```burxt
function map_new<K: Equatable, V>() -> Map<K, V>                    // an empty map
function (mutable self: Map<K, V>) set(key: K, value: V) -> Int      // 1 if new, 0 if it replaced
function (self: Map<K, V>) get(key: K, fallback: V) -> V
function (self: Map<K, V>) find(key: K) -> Option<V>
function (self: Map<K, V>) has(key: K) -> Bool
function (mutable self: Map<K, V>) remove(key: K) -> Bool
function (self: Map<K, V>) count() -> Int
function (self: Map<K, V>) keys() -> [K]
```

Methods rather than free functions, and that was **forced rather than chosen**: Burxt has no
writable parameters, so a container that changes has to change through `mutable self`. The API is
better for it — `counts.set("k", 1)` reads better than `map_set(counts, "k", 1)` — which is the
usual way a real constraint turns out to have been pointing at the nicer design.

`set` answers `1` when the key is new and `0` when it replaced a value, so counting distinct keys
needs no second lookup.

## What is deliberately absent

**No `unwrap`.** Same reason as `lib/option.bx`: it is a decision disguised as a convenience.
`get` covers the case where a default is right and `find` covers the case where it is not.

**No `map`, `filter` or `each`.** They need a function as a value, and a closure needs an owner for
its captured state — a memory question, not a syntax one. Iterate `keys()` with a `for` loop, which
cannot capture anything by accident.

**No HashDoS protection.** `hash` is deterministic and unseeded, because a seeded hash cannot
iterate in a defined order and that is the whole point of the container. A caller feeding
attacker-chosen keys can force collisions and turn O(1) into O(n). If you need that guarded, the
answer will be a second constructor — `map_seeded(seed)` — so the program that needs it says so,
rather than every program paying for it. A security property should be visible in the code that
needs it.

## One thing to know about the shape

A map is two arrays: the entries in insertion order, and an open-addressed table of positions into
them. There is **no per-entry allocation** — which matters more here than in most languages, because
every allocation lands in a region and a region is a bump pointer (page 4). A container that
allocated once per insertion would make the region grow for nothing.

Growing the table re-places the positions and drops the tombstones. It never reorders the entries.

## Next

[Reference](reference.md) — every keyword, builtin, operator and error, in tables.

Or the design record, which carries the reasoning behind every refusal above:
[`spec/M11-MAPS.md`](../../spec/M11-MAPS.md).

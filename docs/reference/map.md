---
layout: doc
title: lib/map.bx
section: reference
description: A key-value table, in insertion order.
---


# `lib/map.bx`

A key-value table, in insertion order.

```burxt
use "lib/map.bx";
```

A LIBRARY file, like lib/option.bx and lib/result.bx. The only compiler support it needs is `hash(x)`; everything else here is ordinary Burxt written with generics. If a map had needed a keyword, the generics would not be real.

Iteration is INSERTION order, always — not "unspecified", not "arbitrary". Go randomises iteration deliberately and Rust randomises its hash seed per process, and both are admissions that hash order leaked into programs that then broke. A language whose thesis is reproducibility should not ship a container whose iteration order depends on a hash function's internals. See spec/M11-MAPS.md Decision 1.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`MapEntry`](#mapentry) | class | One key and its value. `live` is false for a tombstone: a removed entry keeps its place in the entry array so that every |
| [`Map`](#map) | class | Two arrays rather than buckets of lists. |
| [`map_new`](#map-new) | function | An empty map. |
| [`slot_count`](#slot-count) | method on `Map` | Where a key wants to live, as an index into `slots`. |
| [`probe`](#probe) | method on `Map` | The probe sequence for a key, answering the slot index where the key was found, or where it would go if it is not there. |
| [`rehash`](#rehash) | method on `Map` | Grow the slot table and re-place every live entry in it. |
| [`set`](#set) | method on `Map` | Insert, or overwrite what is there. |
| [`has`](#has) | method on `Map` | Whether the key is there. |
| [`get`](#get) | method on `Map` | The value, or the fallback. |
| [`find`](#find) | method on `Map` | The value if it is there, as an `Option<V>`. |
| [`count`](#count) | method on `Map` | — |
| [`keys`](#keys) | method on `Map` | Every live key, in insertion order. The answer is a fresh array, so a caller may iterate it while changing the map — whi |
| [`remove`](#remove) | method on `Map` | Answers whether the key was there to remove. |

## Types
{: #types}

### `MapEntry`
{: #mapentry}

```burxt
class MapEntry<K: Equatable, V> { key: K, value: V, live: Bool }
```

One key and its value. `live` is false for a tombstone: a removed entry keeps its place in the entry array so that everything after it keeps its insertion position, which is the whole point.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L21)

### `Map`
{: #map}

```burxt
class Map<K: Equatable, V> { entries: [MapEntry<K, V>], slots: [Int], live: Int }
```

Two arrays rather than buckets of lists.

entries — insertion order, and therefore iteration order. Holes are tombstoned, never moved. slots   — open addressed, linear probing. Holds an index into `entries` PLUS ONE, so that 0

```burxt
         means empty without a sentinel constant and without a parallel array of flags.
```

live    — how many entries are not tombstones, which is what `count` answers.

This is the compact-table shape, and it is the right one here for a reason particular to Burxt: there is no per-entry allocation at all. Every allocation lands in a region, and a region is a bump pointer, so a container that allocates once per insertion is a container that makes the region grow for no reason. `K: Equatable` is not decoration. It is what justifies `entry.key == key` in `probe` and `hash(key)` alongside it, and Equatable is exactly the set `==` works on — Int, Bool, String and Decimal. A key needs equality and a hash, and the types that have equality are the types that can have one, which is why there is no separate `Hashable` naming the same four.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L38)

## Functions
{: #functions}

### `map_new`
{: #map-new}

```burxt
function map_new<K: Equatable, V>() -> Map<K, V>
```

An empty map.

```burxt
 let mutable counts: Map<String, Int> = map_new();
```

The type arguments come from the ANNOTATION, not from an argument — there is nothing to pass. That works because a call whose type parameters the arguments cannot settle reads them from the expectation, which is strictly better than a turbofish: the type is already written where the value lands, and writing it twice would be the language asking a question it can already answer. Landed in v0.0.116; before that this had to be spelled `Map { entries: [], slots: [], live: 0 }` with all three fields exposed at every construction site. See spec/M11-MAPS.md Decision 7.

Everything else is a METHOD, and not for tidiness: Burxt has no writable parameters, so a container that changes has to change through `mutable self`. The API is better for it — `counts.set("k", 1)` rather than `map_set(counts, "k", 1)` — which is the usual way a real constraint turns out to have been pointing at the nicer design.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L57)

## Methods
{: #methods}

### `slot_count`
{: #slot-count}

```burxt
function (self: Map<K, V>) slot_count() -> Int
```

Where a key wants to live, as an index into `slots`.

The slot table is kept a power of two so that this is a mask rather than a division — except Burxt has no bitwise operators, so it is `remainder`, which is exactly why `hash` clears its sign bit: `remainder` keeps the sign of its left operand, and a negative index is a bounds failure rather than a wrong answer.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L67)

### `probe`
{: #probe}

```burxt
function (self: Map<K, V>) probe(key: K) -> Int
```

The probe sequence for a key, answering the slot index where the key was found, or where it would go if it is not there. Linear probing: cache-friendly, and one sequence to get right rather than two.

The caller must have made room first — a full table would loop forever, and `set` is what guarantees it never is.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L77)

### `rehash`
{: #rehash}

```burxt
function (mutable self: Map<K, V>) rehash(wanted: Int) -> Int
```

Grow the slot table and re-place every live entry in it.

The ENTRY array is not rebuilt and not reordered: that is what keeps insertion order stable across a growth, and it is the difference between this and a plain hash table. Tombstones are dropped here, which is the one moment an entry's index can change — and it changes for every entry at once, so no slot survives to point at the wrong one.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L105)

### `set`
{: #set}

```burxt
function (mutable self: Map<K, V>) set(key: K, value: V) -> Int
```

Insert, or overwrite what is there.

Answers 1 if the key is new and 0 if it replaced a value, so a caller can count distinct keys without a second lookup.

The table grows when it is more than half full. Half rather than the usual seven-eighths because linear probing degrades sharply near full, and the memory is a region's to reclaim in O(1).

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L152)

### `has`
{: #has}

```burxt
function (self: Map<K, V>) has(key: K) -> Bool
```

Whether the key is there.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L179)

### `get`
{: #get}

```burxt
function (self: Map<K, V>) get(key: K, fallback: V) -> V
```

The value, or the fallback.

A fallback rather than an `Option<V>`, and the reason is a current limitation rather than a preference: a generic enum payload must be a scalar, so `Option<Point>` is refused, so an `Option<V>` return would restrict values to scalars — a worse limitation than the one it removes. spec/M11-MAPS.md Decision 5 classes the trigger that would add `find`.

Asking then reading is two lookups. That cost is documented rather than hidden, and it is the same shape lib/option.bx chose: `option_or` and `option_is_some`, and deliberately no `unwrap`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L203)

### `find`
{: #find}

```burxt
function (self: Map<K, V>) find(key: K) -> Option<V>
```

The value if it is there, as an `Option<V>`.

This could not be written until v0.0.118, and the reason was recorded rather than guessed at: a variant payload had to be a scalar, so `Option<Point>` was refused, so an `Option<V>` return would have restricted map VALUES to scalars — a worse limitation than the one it removed. Lifting the payload rule was the trigger spec/M11-MAPS.md Decision 5 named, and this is what it unblocked.

`get` stays, and is still the right call when a default is genuinely right. `find` is for when it is not, and it answers with the one type in this language that cannot be read without saying what happens when there is nothing there.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L232)

### `count`
{: #count}

```burxt
function (self: Map<K, V>) count() -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L251)

### `keys`
{: #keys}

```burxt
function (self: Map<K, V>) keys() -> [K]
```

Every live key, in insertion order. The answer is a fresh array, so a caller may iterate it while changing the map — which a view into the entries could not promise.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L257)

### `remove`
{: #remove}

```burxt
function (mutable self: Map<K, V>) remove(key: K) -> Bool
```

Answers whether the key was there to remove.

The entry is TOMBSTONED, not moved and not deleted, so every entry after it keeps its insertion position. Its slot is left pointing at the tombstone rather than cleared, because clearing it would break the probe chain of any key that collided with it and landed further along.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/map.bx#L277)


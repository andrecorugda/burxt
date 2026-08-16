---
layout: doc
title: lib/set.bx
section: reference
description: "Membership, without a value nobody reads."
---

{% raw %}

# `lib/set.bx`

Membership, without a value nobody reads.

```burxt
use "lib/set.bx";
```

A `Set<T>` answers one question — *is this in here?* — in constant time, and keeps what it was given in the order it was given. `spec/1.0/ROADMAP-1.0.md` §D1g's argument for it is that every comparison language ships one and Burxt had none, so every program that needed to deduplicate a list or remember which ids it had already seen wrote a linear scan over an array.

It is built **over `Map<T, Bool>`**, and nothing here is privileged: `lib/map.bx` does the work, this file spends the value slot on a `true` that is never read. That sounds wasteful and is not the interesting part; the interesting part is that the alternative — calling a `Map<T, Bool>` a set and shipping some free functions — **does not compile.** See the next section, because it is the reason this file has the shape it has.

---- why `Set<T>` is a CLASS, and not a naming convention over `Map` --------------------

§D1g says "over `Map<T, Bool>`" without saying which. Two designs were available:

```burxt
 a wrapper class     class Set<T: Equatable> { held: Map<T, Bool> }, with methods
 free functions      set_add(mutable s: Map<T, Bool>, item), set_has(s, item), ...
```

**The second one is not writable in this language.** Measured, not assumed:

```burxt
 function set_has<T: Equatable>(m: Map<T, Bool>, item: T) -> Bool {
     return m.has(item);
 }
 error: `.has(...)` needs a class value, but this has type Map<T, Bool>.
```

A free generic function's body is checked **abstractly** — `T` never becomes a type, so `Map<T, Bool>` never becomes a class, so it has no methods to call. `lib/array.bx` recorded this exact asymmetry for a different reason: *"A free generic function's body is checked ABSTRACTLY, and a generic method's body never is."* A generic METHOD's body is checked once per instantiation, with `T` already settled, so inside a method `self.held.has(item)` is a call on a real class.

So every operation here is a method, and there is exactly one free function — `set_new` — which gets away with it because its body contains no method call at all. That is also why it builds its map with the field literal `Map { entries: [], slots: [], live: 0 }` rather than calling `map_new()`: inside a generic body `let held: Map<T, Bool> = map_new();` is refused with *"cannot tell what `K` is from this call"* even though the annotation says exactly what `K` is. The literal is the spelling `lib/map.bx` says every construction site needed before v0.0.116, kept alive here for one line. **This is a compiler gap, not a design** — the expectation is right there in the annotation. Reported; if it is fixed, this line becomes `map_new()` and nothing else changes.

---- the order `items()` answers in ------------------------------------------------------

**Insertion order, and a re-added element counts as newly inserted.** `spec/1.0/ROADMAP-1.0.md`'s decisions list says *"no undefined `Map` iteration order, ever"*, so a Set inherits `Map`'s promise rather than inventing one: `Map` iterates in insertion order because Go randomises iteration and Rust randomises its hash seed, and both are admissions that hash order leaked into programs that then broke.

The part worth stating, because it is the part a reader would get wrong: **removing an element and adding it back puts it at the END.**

```burxt
 add 10, 20, 30 · remove 10 · add 10   ->   items() is 20 30 10
```

That is measured on a bare `Map` as well as through this wrapper, so it is `Map`'s behaviour and not something the wrapper introduces. It is worth knowing WHY, because `lib/map.bx`'s own comment says the opposite — it describes `set` "reviving a tombstone in place" and returning 1. That branch **cannot be reached**: `probe` only stops on a slot whose entry is `live && key == key`, so it walks straight past a tombstone and answers with an empty slot, and `set` therefore always takes the append path. The comment describes dead code. Reported rather than edited; `map.bx` belongs to another change.

The upshot is a cleaner guarantee than the one in that comment: an element's position is the position of the `add` that most recently made it a member. Nothing depends on the load factor, and nothing depends on when a rehash happened to drop a tombstone.

---- union / intersect / difference answer a NEW Set ------------------------------------

`lib/array.bx` settled the mutate-or-answer question with `mutable xs: [T]` in the signature and the rule that *"a promise belongs where it is made once"*. These three follow it by having no `mutable` at all: `a.union(b)` changes neither `a` nor `b`, which is what union means everywhere else and what a reader will assume.

**There is also a hard reason, and it is the one that settled it.** The obvious in-place implementation — copy the receiver and add the other side's items to the copy — is **wrong in this language**, because copying a Set does not copy its contents:

```burxt
 let mutable b: Set<Int> = a;   // 12 elements
 let added = b.add(99);         // b has 13, a has 12 ... and a is now CORRUPT
 a.has(99);                     // runtime error: index 12 is out of bounds
```

An array binding is a pointer and a length copied by value while the buffer is shared, so `b` inherited `a`'s slot table, wrote a new entry index into it, and `a`'s entry array never grew to match. Measured on a bare `Map` too, so this is the language's array semantics reaching every container built on one — **not a defect in this file, and not one this file can fix.** Reported.

What it means here: **never copy a Set.** Every operation below that produces a Set builds it with `set_new` and `add`, which is O(n) rather than O(1) and is the honest price of correctness. And the mutating counterparts of the three — "add everything from that one", "keep only what is in that one" — are deliberately NOT called `union`/`intersect`. `add_all` is here because deduplicating a list is the reason most programs want a set at all; `retain` and `remove_all` are not, because nothing has needed them yet and a name that has not been needed is a name to leave unspent.

---- what is NOT here, and why ----------------------------------------------------------

**`==` on a Set is refused by the compiler, so `equals` is a necessity rather than a convenience.** The refusal is worth quoting because the reason is a real undecided question rather than an oversight: *"`==` on `Map<T, Bool>` needs every field to be comparable, and `.entries` is a growable array, and `==` on one is a separate question — two arrays with equal contents and different capacity would have to be equal, and nothing has decided that."* Set equality is not field equality anyway: two sets with the same members inserted in different orders are equal, and their entry arrays are not.

**No `set_from(xs)`.** It would be a free generic function calling a method, which is the thing that does not compile — see above. `add_all` is the replacement and reads no worse: `let n = seen.add_all(ids);`

**No `map` or `filter`.** Function values do not exist; `lib/array.bx` and `lib/map.bx` say the same, and the loop over `items()` is three lines a reader can see.

**No ordered or sorted set.** `items()` is insertion order; for sorted output, `array_sort` the answer. A tree-backed set would be a different container with a different cost and would need a name that says so.

---- how to check whether the four limitations above are still true --------------------

Every claim in this header about what the compiler refuses was measured on **v0.0.249**, and each one is a claim that stops being true the moment somebody fixes it — with nothing linking the fix to this comment. `lib/math.bx` shipped with a comment that had gone stale ninety minutes earlier because A4 landed mid-change, so this block exists to make the claims cheap to re-test rather than expensive to trust.

```burxt
 1. free generic body cannot call a method on `T`'s instantiation
    NO ROADMAP ITEM. Not A3 — A3 is `Option.None` in a free generic function, and it is DONE.
    Its unlocks list names "a generic `Set`", which is this module, and A3 is not what
    unblocked it: a Set in METHOD form needed nothing from A3, and the free-function form is
    still refused. Worth an item; reported so the row can be corrected.
    check: `function f<T: Equatable>(m: Map<T, Bool>, k: T) -> Bool { return m.has(k); }`
```

```burxt
 2. `let m: Map<T, Bool> = map_new();` refused inside a generic body
    Same family as 1, no item. The annotation names `K` and `V` outright.
    check: the line above, inside `function f<T: Equatable>(x: T) -> Int`.
```

```burxt
 3. copying a container shares its buffers
    No item. Array semantics, so it reaches every container; `lib/map.bx` has it too.
    check: fill a Map past 8 entries, `let mutable n = m;`, `n.set(k, v)`, then `m.has(k)`.
```

```burxt
 4. `==` on a growable array is undecided, so `==` on a Set is refused
    No item, and the compiler's own words are *"nothing has decided that"* — a real open
    question rather than a gap. `equals` is the answer either way, because set equality is
    not field equality.
    check: `a == b` on two `Set<Int>`.
```

If one of these now passes, the paragraph above it is the thing to delete — and deleting it should simplify this file rather than only shorten the comment.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Set`](#set) | class | The one field is a `Map<T, Bool>` whose values are all `true`. `T: Equatable` is inherited from `Map` rather than chosen |
| [`set_new`](#set-new) | function | An empty set. |
| [`add`](#add) | method on `Set` | `self.held.set(item, true)` does not compile: *"`set` is a mutating method; it can only be called on a variable, not an  |
| [`add_all`](#add-all) | method on `Set` | Put all of them in, answering **how many were new**. Deduplicating a list is the reason most programs reach for a set, a |
| [`has`](#has) | method on `Set` | The question a set exists to answer. Constant time, not a scan. |
| [`count`](#count) | method on `Set` | How many members. `Map`'s `count` answers live entries, so tombstones are already excluded. |
| [`items`](#items) | method on `Set` | Every member, in insertion order — see the header for what that means after a remove. |
| [`is_subset_of`](#is-subset-of) | method on `Set` | Whether every member of this set is also in `other`. The empty set is a subset of everything, which falls out rather tha |
| [`equals`](#equals) | method on `Set` | Same members, in any order. See the header for why `==` cannot do this. |
| [`remove`](#remove) | method on `Set` | Take one out, answering **whether it was there**. Symmetric with `add`, and for the same reason: the caller usually want |
| [`take`](#take) | method on `Set` | Remove and answer **the first member in insertion order**, or `None` when empty. |
| [`union`](#union) | method on `Set` | Everything in either one. |
| [`intersect`](#intersect) | method on `Set` | Only what is in both. In THIS set's order, which is why `a.intersect(b)` and `b.intersect(a)` hold the same members and  |
| [`difference`](#difference) | method on `Set` | What is in this one and not in the other. Not symmetric, which the name says: `a.difference(b)` is "a without b". |

## Types
{: #types}

### `Set`
{: #set}

```burxt
class Set<T: Equatable> { held: Map<T, Bool> }
```

The one field is a `Map<T, Bool>` whose values are all `true`. `T: Equatable` is inherited from `Map` rather than chosen here, and it is the right bound for the same reason: a member needs equality and a hash, and `Equatable` is exactly the set `==` works on.

`held` is not private, because `private` would put it out of reach of nothing — there is no caller who benefits from reading it, and marking it would only cost the reader a question about why. What keeps a caller from setting a value to `false` and inventing a member that is not one is that no method here ever writes anything but `true`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L165)

## Functions
{: #functions}

### `set_new`
{: #set-new}

```burxt
function set_new<T: Equatable>() -> Set<T>
```

An empty set.

```burxt
 let mutable seen: Set<Int> = set_new();
```

The type argument comes from the ANNOTATION — there is nothing to pass — which is the same mechanism `map_new` uses and the same reason a turbofish would be redundant.

The field literal rather than `map_new()` is the compiler gap described in the header, not a preference. It is also why this is the ONLY free function in the file: no method call in the body means nothing here needs `T` to be a class.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L179)

## Methods
{: #methods}

### `add`
{: #add}

```burxt
function (mutable self: Set<T>) add(item: T) -> Bool
```

`self.held.set(item, true)` does not compile: *"`set` is a mutating method; it can only be called on a variable, not an expression."* A mutating method takes a true reference, so its receiver must be the actual binding — the same rule that governs `item.field = v`. `self.held` is a field access, not a binding.

So the map is lifted into a local, changed there, and written back. The write-back is **required, not defensive**: without it this set stays empty, measured. `let mutable held = self.held` copies the map's three fields, and `held.set` replaces its arrays outright when it rehashes, so the new pointers live only in the local until the last line puts them back.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L201)

### `add_all`
{: #add-all}

```burxt
function (mutable self: Set<T>) add_all(xs: [T]) -> Int
```

Put all of them in, answering **how many were new**. Deduplicating a list is the reason most programs reach for a set, and this is that operation:

```burxt
 let mutable seen: Set<String> = set_new();
 let fresh: Int = seen.add_all(names);      // seen.items() is names without repeats
```

This is also the stand-in for a `set_from(xs)` constructor, which cannot be written — see the header. Two lines at a call site instead of one, and the two lines compile.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L219)

### `has`
{: #has}

```burxt
function (self: Set<T>) has(item: T) -> Bool
```

The question a set exists to answer. Constant time, not a scan.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L232)

### `count`
{: #count}

```burxt
function (self: Set<T>) count() -> Int
```

How many members. `Map`'s `count` answers live entries, so tombstones are already excluded.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L237)

### `items`
{: #items}

```burxt
function (self: Set<T>) items() -> [T]
```

Every member, in insertion order — see the header for what that means after a remove.

A **fresh array**, so a caller may add to or remove from the set while iterating what it answered. A view into the map's entries could not promise that, and the copy is what makes `for i in 0..len(xs)` over a set safe to write without thinking about it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L246)

### `is_subset_of`
{: #is-subset-of}

```burxt
function (self: Set<T>) is_subset_of(other: Set<T>) -> Bool
```

Whether every member of this set is also in `other`. The empty set is a subset of everything, which falls out rather than being special-cased: the loop runs zero times.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L252)

### `equals`
{: #equals}

```burxt
function (self: Set<T>) equals(other: Set<T>) -> Bool
```

Same members, in any order. See the header for why `==` cannot do this.

The count check first is not an optimisation, it is what makes one subset test enough: equal counts plus every member of this one present in that one leaves no room for a member of that one to be missing from this one.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L267)

### `remove`
{: #remove}

```burxt
function (mutable self: Set<T>) remove(item: T) -> Bool
```

Take one out, answering **whether it was there**. Symmetric with `add`, and for the same reason: the caller usually wants to know.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L278)

### `take`
{: #take}

```burxt
function (mutable self: Set<T>) take() -> Option<T>
```

Remove and answer **the first member in insertion order**, or `None` when empty.

This is what makes a Set usable as a **worklist**, which is the shape of every graph walk, every dependency resolution and every queue of things still to visit:

```burxt
 while true {
     match pending.take() {
         None => { break; }
         Some(job) => { ... let more = pending.add_all(next_after(job)); }
     }
 }
```

First rather than arbitrary, deliberately: "some element" is the usual signature elsewhere, and it makes the walk order depend on a hash function — so the same program visits in a different order on a different build, which is the reproducibility this language is for. First-in-first-out costs nothing here because insertion order is already what `items()` answers.

`Option<T>` from a generic method, which has worked since v0.0.118 for `map.find`. It also works from a free generic function as of A3/A4, but that is not needed here — everything is a method.

The cost is honest and worth stating: `items()` builds the whole array to read one element, so a loop draining a set of n members is O(n²). For a worklist of a few thousand that is nothing; for a hot loop over a large set it is the wrong container, and the answer is an array used as a queue with this set beside it for the seen-check.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L309)

### `union`
{: #union}

```burxt
function (self: Set<T>) union(other: Set<T>) -> Set<T>
```

Everything in either one.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L330)

### `intersect`
{: #intersect}

```burxt
function (self: Set<T>) intersect(other: Set<T>) -> Set<T>
```

Only what is in both. In THIS set's order, which is why `a.intersect(b)` and `b.intersect(a)` hold the same members and may list them differently — worth knowing before printing one.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L345)

### `difference`
{: #difference}

```burxt
function (self: Set<T>) difference(other: Set<T>) -> Set<T>
```

What is in this one and not in the other. Not symmetric, which the name says: `a.difference(b)` is "a without b".

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/set.bx#L358)


{% endraw %}

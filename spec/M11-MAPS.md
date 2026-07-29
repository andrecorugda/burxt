# Burxt — Maps (M11)

> Status: **the library works in both compilers (v0.0.115).** `hash` landed in v0.0.114 and
> `lib/map.bx` runs identically under stage-0 and stage-1, including `Map<Int, Int>` through a
> rehash and `Map<String, Point>`. What remains is Acceptance 6 — the compiler using it — and a
> guide page.

## 0. Why this is a milestone and not a nicety

M9 measured the compiler's own performance and wrote down what it would cost later:

> `find_fun`, `find_sym`, `find_type` and the parser's `find_method` each walk a growing array
> per lookup, so the checker is O(n²) in declaration count. **It does not bite yet:** the
> compiler has 40 functions and 896 lookups over 40 entries is nothing. The fix is an index, and
> Burxt has no map type, so that is a feature too.

**The quadratic is real, and it is not what slowed the compiler down.** Both halves of that
sentence are measured, and the second half is a correction to what this spec said when it was
written on 2026-07-29 — which claimed the compiler's 1.67× time growth *was* this quadratic
"arriving exactly where the prediction put it". It is not, and the number that settles it was
already being printed by the compiler:

```
work: 34081 nodes, type_of 22352, find_sym 10644 over 0 syms, find_fun 907 over 39 funs
```

**907 lookups over 39 functions** is about 35,000 comparisons in a 1.96-second compile. Whatever
costs that time, it is not this.

The quadratic itself is easy to see once you look for it in the right place — a generated program
with nothing in it but declarations, timed by stage-1 at v0.0.115:

| Declarations | 400 | 800 | 1600 | 3200 |
|---|---|---|---|---|
| Time | 0.01 s | 0.11 s | 0.63 s | **5.52 s** |

Between 1600 and 3200 the time goes up **8.8×** for 2× the input. That is worse than quadratic, and
5.5 seconds for a program that does nothing is a wall any real codebase will hit. It is worth
fixing. It is simply not the compiler's own problem, because the compiler has 39 top-level
functions and not 3200.

**What does cause the compiler's 1.67× is unattributed**, and saying so is the point. M9's own rule
is that three of its four guesses were wrong and the numbers were the only reason that was cheap.
Repeating a prediction as a finding is exactly the mistake it warns about, and this spec made it
one commit after re-measuring. The honest next step for that question is a controlled experiment,
not another guess.

## 1. The shape, and what it deliberately is not

**A map is a library, not a keyword.** `lib/map.bx`, alongside `lib/option.bx` and
`lib/result.bx`, and for the same reason: if a map needed compiler support beyond one primitive,
the generics would not be real. The one primitive it needs is `hash`.

### Decision 1 — iteration order is INSERTION order, always

Not "unspecified", not "arbitrary", not "don't rely on it". Defined.

This is the decision that separates a Burxt map from the textbook one, and the argument is not
taste. Two major languages shipped hash-ordered iteration and both had to walk it back: Go
**randomises** iteration deliberately so that nobody can depend on it, and Rust **randomises the
hash seed per process** so that iteration order differs between runs. Both are admissions that
hash order leaked into programs that then broke.

Burxt's thesis is exactness and reproducibility — the same inputs producing the same bytes, which
is what the byte-identical self-hosting fixpoint is *for*. A container whose iteration order
depends on a hash function's internals is a determinism hazard sitting in the middle of that.
Printing a map, serialising one, or hashing a structure that contains one all become
run-dependent, and none of those should be.

The cost is real and worth stating: insertion order means a deleted key leaves a hole to
tombstone rather than a slot to reuse freely, and iteration walks the entry array rather than the
slot table. That is a constant factor. Determinism is worth a constant factor.

### Decision 2 — the key bound is `Equatable`, which already exists

```burxt
record Map<K: Equatable, V> { ... }        // used as Map<String, Int>
```

The bound is not decoration. It is what justifies `entry.key == key` inside `probe` and `hash(key)`
beside it — and it has to be checked in the DECLARATION, because a declaration that cannot be
justified is wrong whether or not anyone instantiates it. Stage-1 checks it there and stage-0 does
not, since stage-0 only ever checks instantiated copies where `K` is already a concrete type. Two
compilers, and the stricter one was right.

`Equatable` means `Int`, `Decimal`, `Bool`, `String` — **exactly the types `==` works on**, which
is the rule §"Bounds" in the guide already states: a bound cannot promise more than the language
delivers. A map key needs equality and a hash, and the set of types that have equality is the set
that can have a hash. No new concept is introduced, and there is no `Hashable` trait, because it
would name the same set twice.

A record as a key is refused. It would need structural hashing, which needs a per-type walk, which
needs either a derive mechanism or a trait with a method — and both are larger than this milestone.
**Trigger:** a program that genuinely wants a compound key, at which point the honest answer is
probably a `String` built from the parts, and if that is not enough, a `Hashable` trait with one
method.

### Decision 3 — the layout is a compact ordered table

```burxt
record MapEntry<K: Equatable, V> { key: K, value: V, live: Bool }

record Map<K: Equatable, V> {
    entries: [MapEntry<K, V>],   // insertion order, the iteration order, holes tombstoned
    slots:   [Int],              // open addressed: index-into-entries PLUS ONE, 0 means empty
    live:    Int                 // how many entries are not tombstones
}
```

Two arrays rather than buckets-of-lists. This is Python's post-3.6 compact dict, and it is the
right modern answer for the same reasons it was there: one contiguous entry array means iteration
is a linear walk with no pointer chasing, insertion order is free rather than bolted on, and there
is **no per-entry allocation** — which matters more here than in Python, because every allocation
lands in a region and a region is a bump pointer.

`slots` holds *index plus one* so that `0` can mean empty without a sentinel constant and without
a parallel array of flags. Linear probing, because it is cache-friendly and because a second probe
sequence is a second thing to get wrong.

A growable array of a generic record is exactly what this needs, and it is the shape that turned
out to be broken in stage-1 when this milestone was scoped — `push(table, Entry { ... })` never
told the literal what it was. Fixed in v0.0.114, with `tests/pass/generics_in_arrays.bx` pinning
it, before any of this was built on top.

### Decision 4 — `hash` is deterministic and unseeded

`hash(x) -> Int`, over `Equatable`. **FNV-1a** over the bytes for a `String`; a multiplicative
mix for `Int`, `Bool` and `Decimal`. The same input gives the same hash in every run, on every
machine, forever.

That is a deliberate trade and it must be said plainly rather than discovered: **there is no
HashDoS protection.** A caller who feeds attacker-chosen keys into a map can force collisions and
turn O(1) into O(n). Rust seeds randomly precisely to stop this.

Why Burxt takes the other side, for now: a seed makes `hash` non-reproducible, which breaks
Decision 1, which is the reason the map exists in this shape. And the first user of this map is a
**compiler**, whose keys are identifiers in a source file the user already chose to compile.

**Trigger that changes this:** a Burxt program serving untrusted input where keys come from the
request. At that point the answer is not a random seed — that would surrender determinism
globally for one caller's problem — but a *second* constructor, `map_seeded(seed)`, so the program
that needs it says so, and the seed is an input like any other. Which is the more honest shape
anyway: a security property should be visible in the code that needs it.

### Decision 5 — lookup takes a fallback; there is no `unwrap`

Methods, not free functions, and that was forced rather than chosen: **Burxt has no writable
parameters**, so a container that changes has to change through `mutable self`. The API is better
for it — `counts.set("k", 1)` reads better than `map_set(counts, "k", 1)` — which is the usual way
a real constraint turns out to have been pointing at the nicer design.

```burxt
function (mutable self: Map<K, V>) set(key: K, value: V) -> Int     // 1 if new, 0 if it overwrote
function (self: Map<K, V>) get(key: K, fallback: V) -> V            // the value, or the fallback
function (self: Map<K, V>) has(key: K) -> Bool
function (mutable self: Map<K, V>) remove(key: K) -> Bool           // whether it was there
function (self: Map<K, V>) count() -> Int
function (self: Map<K, V>) keys() -> [K]                            // in insertion order
```

`get(k, fallback)` and not `get(k) -> Option<V>`, for one concrete reason: a generic enum payload
must currently be a scalar, so `Option<Point>` is refused, so an `Option<V>` return would restrict
map VALUES to scalars — a worse limitation than the one it removes.

`has` then `get` is two lookups, and the cost is documented rather than hidden. It is also the same
shape `lib/option.bx` chose (`option_or`, `option_is_some`, and deliberately no `unwrap`), so a
reader learns one idiom and not two.

**Trigger for `Option<V>`:** lifting the scalar-payload restriction on generic enums. Then `find`
can be added alongside, and `get` stays for the common case.

### Decision 6 — no `map`, `filter` or `each`

They need a function as a value, and a closure needs an owner for its captured state, which is a
memory question and not a syntax one. Iteration is a `for` loop over `map_keys`, which is a
construct the language already has and which cannot capture anything by accident.

### Decision 7 — `map_new()` exists, and closing the two gaps that blocked it was the real work

```burxt
let mutable counts: Map<String, Int> = map_new();
```

This spec first recorded that a constructor **could not be written**, and named two holes. Both are
closed in v0.0.116, and they were worth closing for every generic anyone writes, not just this one:

1. **A generic record literal inside the generic that declares it.** `Map { entries: [], ... }` in
   `map_new`'s body is not an instantiation of anything yet — it becomes one when `map_new` does.
   `expand` already left it abstract by the same rule the function path uses, and
   `instantiate_record` called that "codegen bug". It now answers the **application** rather than a
   bare name, so it still equals the `Map<K, V>` the signature wrote, and nothing lowers it because
   `specialise` clones the untyped declaration and the copy is checked fresh. **The abstract pass
   validates; the concrete pass compiles.**
2. **A type parameter with nothing to infer from.** Read from the **expectation**:
   `let m: Map<String, Int> = map_new();` already says what `K` and `V` are, in the place this
   language says a type belongs. That is strictly better than a turbofish, which would be the
   language demanding an answer it is already holding — and it keeps the guide's "there is no
   turbofish" true rather than adding a footnote to it.

A bound travels with the parameter too: `satisfies` now accepts a type parameter whose own
declaration carries the bound, because a generic that declares `K: Equatable` and then cannot rely
on it has a bound for decoration.

**Three defects fell out of building it, all in stage-1**, and one of them was old:

- The call's answer was resolved **shallowly**, so a function returning `Map<K, V>` answered
  `Map<K, V>` with the parameters still in it. The emitter reads that cached answer to choose which
  copy to call, so a half-resolved one meant a call to an unmangled symbol nothing defines.
- `unify` descended into slices but **not into a named application's arguments**, so a parameter
  reachable only through a type argument could not be inferred — which is every constructor.
- **`Box<Box<Int>>` read its argument as `Int`.** The parser took `args_start` *before* parsing the
  arguments, and a nested application pushes its own arguments while the outer one is still
  parsing, so the outer range began inside the inner list. Gathered-then-appended now, which is the
  `commit(base)` shape every other nested list in that parser already uses. This was wrong from the
  day applications were parsed and no test had reached it.

`tests/pass/generics_constructors.bx` pins all of it, including the nested case and a
zero-argument constructor.

## 2. What has to change in the compilers

Only `hash`. Everything else is Burxt.

| | Stage-0 (Rust) | Stage-1 (Burxt) |
|---|---|---|
| Reserved name | `is_reserved_name`-equivalent list | `is_reserved_name` in check.bx |
| Arity and result | the builtin dispatch in `typeck.rs` | `builtin_arity`, `builtin_result` in parser.bx |
| Argument rule | `Equatable` only, named error | `check_builtin_args` in parser.bx |
| Lowering | `codegen.rs` | one runtime helper in `emit.bx`'s `runtime_ir()` |

The lowering is **one helper written once**, not a loop emitted per call site — the same choice
`write_bytes` made in v0.0.113 and for the same reason.

## 3. Acceptance

1. `hash` exists in both compilers, refuses a record with a message naming why, and the two
   compilers produce **the same hash for the same input** — a differential fixture, because a map
   that hashes differently in the two compilers would put the same key in two places.
2. `lib/map.bx` compiles under both, with no compiler support beyond `hash`.
3. A pass fixture covering insert, overwrite, get, has, remove, count, and **iteration order after
   a removal** — the case Decision 1 is about, and the one a hash-ordered map gets wrong.
4. `Map<String, Point>` works. A record VALUE is the case Decision 5 exists to protect.
5. A fail fixture per refusal: a record as a key, and `hash` of a record.
6. **A program with thousands of declarations gets dramatically faster.** ✅ **5.52 s → 0.33 s**
   for 3200 declarations in v0.0.117, a 16.7× improvement, guarded by a ratio in
   `the_compiler_compiles_itself_without_going_quadratic`.

   The cause was measured, not guessed: **declaring** a function looks it up first, to refuse a
   duplicate, so declaring n of them scanned a growing table n times. The fix is a hash index over
   the name spans — chained buckets, `span_hash` computed **over the bytes where they are**, because
   `hash(substring(...))` would allocate once per lookup in a region and trade a quadratic for a
   leak.

   Not `lib/map.bx` itself, and that is the honest outcome: the compiler keys by a span into its
   source, and a `Map<String, Int>` would need a String built per lookup. The map earned its place
   as a language feature; the compiler needed the same idea with no allocation. Both are in this
   milestone and only one is a library.

   **What did NOT get fixed, measured:** the ratio is still ~16× for 4× the input, because 4× the
   declarations is also 4× the bytes and the front end has an older quadratic in input size — `len`
   on a String is `strlen`. M9 §3 named it and its fix is a milestone of its own. Chasing it here
   would have been a second milestone smuggled into this one; the ratchet's comment says exactly
   which quadratic it still tolerates and what the bar should become when that one goes.
7. The fixpoint still holds, byte for byte, and the backend equality stays at all-of-them.
8. A guide page, because a container people will reach for daily is not documented by a spec.

## 4. What this must NOT do

- **NO undefined iteration order**, ever, for any performance argument. That is Decision 1 and it
  is the whole reason this is not just "add a hash map".
- **NO random seed by default.** See Decision 4 — the trigger is a second constructor, not a
  global change.
- **NO `unwrap`.** Same reason as `lib/option.bx`: it is a decision disguised as a convenience.
- **NO growing the language.** If a map needs a keyword, the generics are not real, and the fix is
  the generics rather than the keyword.
- **NO guessing at the performance win.** §0 has a before number. Acceptance 6 needs an after one.

# Burxt — Maps (M11)

> Status: **specified, in progress.** The `hash` builtin and `lib/map.bx` are the work; the
> compiler's own `find_fun`/`find_sym`/`find_type` are the acceptance test.

## 0. Why this is a milestone and not a nicety

M9 measured the compiler's own performance and wrote down what it would cost later:

> `find_fun`, `find_sym`, `find_type` and the parser's `find_method` each walk a growing array
> per lookup, so the checker is O(n²) in declaration count. **It does not bite yet:** the
> compiler has 40 functions and 896 lookups over 40 entries is nothing. The fix is an index, and
> Burxt has no map type, so that is a feature too.

It bites now. Re-measured at v0.0.110: the compiler's source grew **1.29×** since v0.0.90 while
its self-compile time grew **1.67×**. Memory grew 1.22× — linear, healthy — so the extra time is
not allocation. It is the predicted O(n²), arriving exactly where the prediction put it.

So this milestone has a number attached to it before a line is written, which is the shape M9
argued every performance change should have.

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
record Map<K, V> { ... }        // used as Map<String, Int>
```

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

```
record Entry<K, V> { key: K, value: V, live: Bool }

record Map<K, V> {
    entries: [Entry<K, V>],   // insertion order, the iteration order, holes tombstoned
    slots:   [Int],           // open-addressed: index-into-entries PLUS ONE, 0 means empty
    live:    Int              // how many entries are not tombstones
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

```burxt
function map_set(...)  -> Int          // insert or overwrite
function map_get(...)  -> V            // the value, or the fallback given
function map_has(...)  -> Bool         // whether the key is there
function map_remove(...) -> Bool       // whether it was there to remove
function map_count(...) -> Int
function map_keys(...) -> [K]          // in insertion order
```

`map_get(m, k, fallback)` and not `map_get(m, k) -> Option<V>`, for one concrete reason: a generic
enum payload must currently be a scalar, so `Option<Point>` is refused, so an `Option<V>` return
would restrict values to scalars — a worse limitation than the one it removes.

The pair `map_has` then `map_get` is two lookups, and the cost is documented rather than hidden.
It is also the same shape `lib/option.bx` chose (`option_or`, `option_is_some`, and deliberately no
`unwrap`), so a reader learns one idiom and not two.

**Trigger for `Option<V>`:** lifting the scalar-payload restriction on generic enums. Then
`map_find` can be added alongside, and `map_get` stays for the common case.

### Decision 6 — no `map`, `filter` or `each`

They need a function as a value, and a closure needs an owner for its captured state, which is a
memory question and not a syntax one. Iteration is a `for` loop over `map_keys`, which is a
construct the language already has and which cannot capture anything by accident.

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
6. **The compiler uses it.** `find_fun`, `find_sym`, `find_type` and `find_method` become lookups,
   and the self-compile is re-measured. This is the acceptance test M9 wrote in advance, so the
   milestone is not done until there is a before-and-after number.
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

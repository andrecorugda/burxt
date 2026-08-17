---
layout: doc
title: lib/vector.bx
section: reference
description: "Vector similarity, EXACTLY."
---

{% raw %}

# `lib/vector.bx`

Vector similarity, EXACTLY.

```burxt
use "lib/vector.bx";
```

The same query returns byte-identical scores on every CPU, every target and every run — and the compiler stops rather than silently losing a digit. See spec/N9-VECTORS-EXACTLY.md.

---- Why that is not a claim any other vector store can make ----------------------------

`f32` addition is **not associative**. `(a+b)+c` and `a+(b+c)` differ in the last bits, so a dot product's answer depends on the order the SIMD lanes happened to reduce in — which depends on the CPU, the compiler version and the thread count. Every production vector database therefore has scores that wobble in the last digits between machines, and nobody treats it as a bug because no alternative is available to them.

Scaled-integer arithmetic IS associative and exact. The wobble is not reduced, it is absent.

What that buys, and it is more than tidiness: a ranking that cannot silently reorder near-ties, a retrieval test that asserts a SCORE rather than a range, and an audit trail you can re-verify on different hardware a year later. Nobody can do the second one today, which is why RAG quality is measured statistically rather than pinned.

---- The scale, and why it is 7 ---------------------------------------------------------

A component of an embedding lives in [-1, 1], so at scale S its unscaled integer is at most 10^S. A product of two lands at scale 2S. Summing D of them needs

```burxt
 D × 10^(2S)  <  2^63 ≈ 9.22 × 10^18
```

```burxt
 Decimal<6>  → products at 12  → ~9,200,000 dimensions
 Decimal<7>  → products at 14  → ~92,000 dimensions      ← every real embedding size
 Decimal<8>  → products at 16  → ~920 dimensions         ← 1536 OVERFLOWS
```

So `Decimal<7>` in, `Decimal<14>` out, and the wall is a TRAP rather than a wrap: at scale 8 a 1536-dimension dot product answers

```burxt
 burxt runtime error: arithmetic overflow — the exact result no longer fits in the value range
```

which is the property. This library either answers exactly or refuses to answer. There is no third outcome where it answers approximately without saying so.

`Decimal<7>` also carries MORE component precision than `f32` does near 1.0 — f32's spacing there is about 6×10⁻⁸ — so exactness is close to free rather than bought.

---- What is NOT here yet, and why ------------------------------------------------------

**`vector_normalise` is absent, but no longer blocked.** It used to be: dividing a `Decimal<7>` needs a rounding contract, so a normalised component is a `Decimal<7, RoundHalfEven>`, and `push` would not take a plain `Decimal<7>` into a contracted array — so neither the plain API nor the contracted one could be written. That second half turned out to be a bug rather than a rule, fixed at all eight declared positions in v0.0.194, and the division-based version now compiles.

It is still not urgent. `vector_dot` and `vector_squared_distance` need no normalisation at all, and the major embedding providers already return unit-length vectors — so in the common case cosine IS the dot product. `vector_magnitude` is here, exact, for checking that.

The open question is which TYPE it should answer, and it is a real one rather than a gap: a `[Decimal<7, RoundHalfEven>]` cannot be handed to `vector_dot` above, because dropping the contract at the element type of a whole slice is refused — and rightly, since a slice is aliasable and the callee could write a rounded value back. So a contracted return type would split this API in two.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Scored`](#scored) | class | One scored candidate. A class rather than two parallel arrays, because a score and the row it came from travelling toget |
| [`Row`](#row) | class | A stored vector: an identifier and its components. |
| [`vector_dot`](#vector-dot) | function | The dot product — the inner-product metric, and cosine similarity when both vectors are unit length. |
| [`vector_squared_distance`](#vector-squared-distance) | function | Squared Euclidean distance. **No square root**, and none needed: `√` is monotonic, so squared distance ranks identically |
| [`vector_magnitude_squared`](#vector-magnitude-squared) | function | — |
| [`vector_magnitude`](#vector-magnitude) | function | The magnitude, floored to seven places — a stated contract rather than a hidden approximation, so the same input gives t |
| [`magnitude_of_squared`](#magnitude-of-squared) | function | The same search, asked about a squared value rather than an array — because `vector_normalise` needs the magnitude of a  |
| [`vector_is_unit`](#vector-is-unit) | function | Is this vector unit length, to seven places? Worth asking rather than assuming, because "the provider returns normalised |
| [`vector_normalise`](#vector-normalise) | function | About 1.5e-7 per component: 1e-7 from the floored magnitude and up to 5e-8 from landing on the grid. Deterministic, whic |
| [`vector_top_dot`](#vector-top-dot) | function | The `count` best matches by DOT PRODUCT, highest first. |
| [`component_to_json`](#component-to-json) | function | The digits of a `Decimal<7>` as a JSON string. `to_string` already renders every place, so this is the whole conversion. |
| [`component_from_json`](#component-from-json) | function | A `Decimal<7>` back from its digits, or None when they are not exactly that. |
| [`vector_to_json`](#vector-to-json) | function | — |
| [`vector_from_json`](#vector-from-json) | function | A row back from one JSON object. Every failure says which row and why — a store that skips a line it could not read is a |
| [`vector_store_render`](#vector-store-render) | function | The whole store as JSONL text. Kept separate from writing it, so a caller can put it anywhere — stdout, a socket once th |
| [`vector_store_parse`](#vector-store-parse) | function | — |
| [`vector_store_write`](#vector-store-write) | function | — |
| [`vector_store_read`](#vector-store-read) | function | — |
| [`vector_store_append`](#vector-store-append) | function | Append one row without rewriting the file, which is the reason the format is JSONL. |

## Types
{: #types}

### `Scored`
{: #scored}

```burxt
class Scored { at: Int, score: Decimal<14> }
```

One scored candidate. A class rather than two parallel arrays, because a score and the row it came from travelling together is a thing worth naming.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L269)

### `Row`
{: #row}

```burxt
class Row { id: String, values: [Decimal<7>] }
```

A stored vector: an identifier and its components.

Declared after the functions that use it on purpose — Burxt collects every declaration before checking any body, so ordering is a matter of what reads best rather than a constraint. `Row` reads best beside the store it belongs to.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L322)

## Functions
{: #functions}

### `vector_dot`
{: #vector-dot}

```burxt
pure function vector_dot(a: [Decimal<7>], b: [Decimal<7>]) -> Decimal<14>
```

The dot product — the inner-product metric, and cosine similarity when both vectors are unit length.

`requires len(a) == len(b)` is the dimension contract. It is not defensive: two vectors of different length have no dot product, and answering one anyway by stopping at the shorter is the class of quiet nonsense that a precondition exists to refuse.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L75)

### `vector_squared_distance`
{: #vector-squared-distance}

```burxt
pure function vector_squared_distance(a: [Decimal<7>], b: [Decimal<7>]) -> Decimal<14>
```

Squared Euclidean distance. **No square root**, and none needed: `√` is monotonic, so squared distance ranks identically to distance. Every nearest-neighbour search that only needs an ORDER can use this and stay exact.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L90)

### `vector_magnitude_squared`
{: #vector-magnitude-squared}

```burxt
pure function vector_magnitude_squared(a: [Decimal<7>]) -> Decimal<14>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L103)

### `vector_magnitude`
{: #vector-magnitude}

```burxt
pure function vector_magnitude(a: [Decimal<7>]) -> Decimal<7>
```

The magnitude, floored to seven places — a stated contract rather than a hidden approximation, so the same input gives the same answer forever.

Found by BINARY SEARCH over the scaled count, comparing squares, which is the whole trick: nothing here converts a Decimal to an Int, because Burxt has no way to do that (`as scaled` is FFI-only). `tick * n` builds a `Decimal<7>` from a count of ten-millionths — the same penny-times-a-count shape `lib/json.bx` uses one scale up — and `candidate * candidate` lands at scale 14, exactly where the target already is. So the comparison is between two exact values of the same type.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L115)

### `magnitude_of_squared`
{: #magnitude-of-squared}

```burxt
pure function magnitude_of_squared(squared: Decimal<14>) -> Decimal<7>
```

The same search, asked about a squared value rather than an array — because `vector_normalise` needs the magnitude of a vector it never builds.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L121)

### `vector_is_unit`
{: #vector-is-unit}

```burxt
pure function vector_is_unit(a: [Decimal<7>]) -> Bool
```

Is this vector unit length, to seven places? Worth asking rather than assuming, because "the provider returns normalised vectors" is a claim about someone else's code.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L139)

### `vector_normalise`
{: #vector-normalise}

```burxt
pure function vector_normalise(a: [Decimal<7>]) -> [Decimal<7>]
```

About 1.5e-7 per component: 1e-7 from the floored magnitude and up to 5e-8 from landing on the grid. Deterministic, which is the claim — the same input gives the same unit vector on every machine and every run, and a deterministic 1.5e-7 is a different kind of thing from f32's nondeterministic one.

Measured over 399 two-dimensional vectors spanning three orders of magnitude, the boost is the difference between a worst case of **1.0000411** and one of **1.0000001** — and between 75 of them reading exactly unit and 290 of them doing so.

A consequence worth knowing before it surprises you: **`vector_is_unit` sometimes answers false on a freshly normalised vector — about a quarter of the time — and that is the right answer.** It asks whether the magnitude reads exactly 1 at seven places, which is a window one grid step wide, and rounding components onto that same grid moves the magnitude by about one step. It is not a defect in either function; it is what "unit length on a finite grid" means, and no amount of care removes it. Use the result for `vector_dot`, which is what it is for; do not use `vector_is_unit` as an acceptance test after normalising.

The precondition is the zero vector, which has no direction to point in. `vector_magnitude_squared` rather than `vector_magnitude` because it is one dot product against thirty binary-search steps, and it is exactly the right question: a vector is normalisable iff its squared magnitude is nonzero.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L204)

### `vector_top_dot`
{: #vector-top-dot}

```burxt
pure function vector_top_dot(rows: [Row], query: [Decimal<7>], count: Int) -> [Scored]
```

The `count` best matches by DOT PRODUCT, highest first.

Brute force, and that is deliberate for this slice: it is exact by construction and it is the baseline any index has to be checked against. An approximate index may come later and may still score exactly — approximate CANDIDATE selection, exact SCORING — which is the split that keeps the claim while getting the speed.

Insertion into a kept list rather than sort-then-take, so nothing needs a generic `sort` — and `xs[i] = v` does not work through a `[T]` today, which is what a generic sort would need.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L280)

### `component_to_json`
{: #component-to-json}

```burxt
pure function component_to_json(value: Decimal<7>) -> Json
```

The digits of a `Decimal<7>` as a JSON string. `to_string` already renders every place, so this is the whole conversion.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L338)

### `component_from_json`
{: #component-from-json}

```burxt
pure function component_from_json(value: Json) -> Option<Decimal<7>>
```

A `Decimal<7>` back from its digits, or None when they are not exactly that.

**It never rounds.** `"0.12345678"` answers None rather than `0.1234568`, because a component arriving with more precision than the store holds is a question and not a rounding — the writer meant those digits. Same rule as `json_as_money`, one scale up.

The reconstruction is a count of ten-millionths times one ten-millionth, exact by construction and needing no rounding contract, because that is what a scaled decimal already is.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L350)

### `vector_to_json`
{: #vector-to-json}

```burxt
pure function vector_to_json(row: Row) -> Json
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L393)

### `vector_from_json`
{: #vector-from-json}

```burxt
pure function vector_from_json(value: Json) -> Result<Row, String>
```

A row back from one JSON object. Every failure says which row and why — a store that skips a line it could not read is a store that quietly returns fewer results.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L408)

### `vector_store_render`
{: #vector-store-render}

```burxt
pure function vector_store_render(rows: [Row]) -> String
```

The whole store as JSONL text. Kept separate from writing it, so a caller can put it anywhere — stdout, a socket once there is one, a test's assertion.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L447)

### `vector_store_parse`
{: #vector-store-parse}

```burxt
function vector_store_parse(text: String) -> Result<[Row], String>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L457)

### `vector_store_write`
{: #vector-store-write}

```burxt
function vector_store_write(path: String, rows: [Row]) -> Int touches files
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L485)

### `vector_store_read`
{: #vector-store-read}

```burxt
function vector_store_read(path: String) -> Result<[Row], String> touches files
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L489)

### `vector_store_append`
{: #vector-store-append}

```burxt
function vector_store_append(path: String, row: Row) -> Int touches files
```

Append one row without rewriting the file, which is the reason the format is JSONL.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/vector.bx#L494)


{% endraw %}

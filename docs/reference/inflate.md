---
layout: doc
title: lib/inflate.bx
section: reference
description: "Read a DEFLATE stream, RFC 1951."
---

{% raw %}

# `lib/inflate.bx`

Read a DEFLATE stream, RFC 1951.

```burxt
use "lib/inflate.bx";
```

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Inflater`](#inflater) | class | `at` is the byte the next bits come from; `bits`/`nbits` hold what has been pulled off it and not yet consumed. A decode |
| [`inflate_bits`](#inflate-bits) | function | `count` bits, least significant first. A read past the end sets `fault` and answers 0 rather than trapping, because a tr |
| [`inflate_build`](#inflate-build) | function | Build `counts` and `symbols` from a list of code lengths. Answers 0 for a complete code, a positive number for an incomp |
| [`inflate_decode`](#inflate-decode) | function | One symbol, most significant bit first. -1 on a code that is not in the tree. |
| [`inflate_tables`](#inflate-tables) | function | The same numbers `lib/deflate.bx` writes, read back. Duplicated rather than shared because the two modules are independe |
| [`inflate_codes`](#inflate-codes) | function | Decode literals and matches until the end-of-block symbol. **A match may reach into bytes this call just wrote** — a dis |
| [`inflate_fixed`](#inflate-fixed) | function | — |
| [`inflate_dynamic`](#inflate-dynamic) | function | The one zlib actually writes. Three counts, then 19 code lengths **in a permuted order**, then the literal and distance  |
| [`inflate_stored`](#inflate-stored) | function | BTYPE 00: the bit stream abandons its partial byte, then a length and its one's complement. |
| [`inflate_into`](#inflate-into) | function | A raw deflate stream starting at `from`. Answers how many bytes were written, or **-1 for a stream this module refuses** |
| [`zlib_into`](#zlib-into) | function | A **zlib** stream — two header bytes and a trailing adler32 — which is what a PNG's IDAT holds and what a ZIP entry does |

## Types
{: #types}

### `Inflater`
{: #inflater}

```burxt
class Inflater
```

`at` is the byte the next bits come from; `bits`/`nbits` hold what has been pulled off it and not yet consumed. A decoder is nothing but this plus two tables.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/inflate.bx#L57)

## Functions
{: #functions}

### `inflate_bits`
{: #inflate-bits}

```burxt
function inflate_bits(mutable state: Inflater, data: [Int], count: Int) -> Int
```

`count` bits, least significant first. A read past the end sets `fault` and answers 0 rather than trapping, because a truncated stream is a thing to report and not a thing to crash on: this module reads bytes somebody else wrote.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/inflate.bx#L69)

### `inflate_build`
{: #inflate-build}

```burxt
function inflate_build(lengths: [Int], mutable counts: [Int], mutable symbols: [Int]) -> Int
```

Build `counts` and `symbols` from a list of code lengths. Answers 0 for a complete code, a positive number for an incomplete one, and -1 for over-subscribed — which is a malformed stream rather than a bug here, so it is a value and not a trap.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/inflate.bx#L91)

### `inflate_decode`
{: #inflate-decode}

```burxt
function inflate_decode(mutable state: Inflater, data: [Int], counts: [Int], symbols: [Int]) -> Int
```

One symbol, most significant bit first. -1 on a code that is not in the tree.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/inflate.bx#L146)

### `inflate_tables`
{: #inflate-tables}

```burxt
function inflate_tables(mutable length_base: [Int], mutable length_extra: [Int],
```

The same numbers `lib/deflate.bx` writes, read back. Duplicated rather than shared because the two modules are independent by design — and because a decoder importing the encoder to borrow a table would mean a program that only reads streams carries a compressor it never calls.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/inflate.bx#L173)

### `inflate_codes`
{: #inflate-codes}

```burxt
function inflate_codes(mutable state: Inflater, data: [Int], mutable out: [Int],
```

Decode literals and matches until the end-of-block symbol. **A match may reach into bytes this call just wrote** — a distance smaller than the length is legal and is how a run of one byte is encoded — so the copy is byte at a time from the growing output rather than a block move.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/inflate.bx#L203)

### `inflate_fixed`
{: #inflate-fixed}

```burxt
function inflate_fixed(mutable state: Inflater, data: [Int], mutable out: [Int],
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/inflate.bx#L252)

### `inflate_dynamic`
{: #inflate-dynamic}

```burxt
function inflate_dynamic(mutable state: Inflater, data: [Int], mutable out: [Int],
```

The one zlib actually writes. Three counts, then 19 code lengths **in a permuted order**, then the literal and distance lengths decoded with that tree — including three run-length symbols, one of which repeats the PREVIOUS length and therefore cannot be the first thing in the list.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/inflate.bx#L285)

### `inflate_stored`
{: #inflate-stored}

```burxt
function inflate_stored(mutable state: Inflater, data: [Int], mutable out: [Int]) -> Int
```

BTYPE 00: the bit stream abandons its partial byte, then a length and its one's complement.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/inflate.bx#L386)

### `inflate_into`
{: #inflate-into}

```burxt
function inflate_into(mutable out: [Int], data: [Int], from: Int) -> Int
```

A raw deflate stream starting at `from`. Answers how many bytes were written, or **-1 for a stream this module refuses** — truncated, over-subscribed, a distance past the start of the output, a reserved block type. A refusal is a value rather than a trap because these are somebody else's bytes and a caller must be able to say "not this one" without dying.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/inflate.bx#L419)

### `zlib_into`
{: #zlib-into}

```burxt
function zlib_into(mutable out: [Int], data: [Int]) -> Int
```

A **zlib** stream — two header bytes and a trailing adler32 — which is what a PNG's IDAT holds and what a ZIP entry does NOT. The checksum is verified: a decoder that ignores it hands back bytes that failed their own check, which is the same defect as an oracle reading one copy of duplicated metadata.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/inflate.bx#L465)


{% endraw %}

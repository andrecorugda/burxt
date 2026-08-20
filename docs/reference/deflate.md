---
layout: doc
title: lib/deflate.bx
section: reference
description: "DEFLATE compression, RFC 1951."
---

{% raw %}

# `lib/deflate.bx`

DEFLATE compression, RFC 1951.

```burxt
use "lib/deflate.bx";
```

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Deflater`](#deflater) | class | The window is 32 KB because RFC 1951 fixes it there: a distance is at most 32,768, so a match further back cannot be exp |
| [`deflate_put_bits`](#deflate-put-bits) | function | `count` bits of `value`, least significant first. Every field RFC 1951 calls a "data element" goes through here: the blo |
| [`deflate_put_code`](#deflate-put-code) | function | A Huffman code, most significant bit first — the opposite order, per §3.1.1. Written a bit at a time rather than by reve |
| [`deflate_flush`](#deflate-flush) | function | Pad the final byte with zeroes. A deflate stream ends on a bit boundary that is rarely a byte boundary, and the padding  |
| [`deflate_put_symbol`](#deflate-put-symbol) | function | RFC 1951 §3.2.6, written out rather than derived. The four ranges are the specification's, and the codes are the canonic |
| [`deflate_fill_tables`](#deflate-fill-tables) | function | Filled by a method rather than declared at the top level, because a top-level `let` is not visible inside a function bod |
| [`deflate_put_length`](#deflate-put-length) | function | A length becomes the largest code whose base does not exceed it, plus the difference in extra bits. Walked from the top  |
| [`deflate_put_distance`](#deflate-put-distance) | function | A distance's code comes from the same shape of table, but its code goes through the fixed DISTANCE tree, which is five b |
| [`deflate_hash`](#deflate-hash) | function | Three bytes into one of 32,768 buckets. Any hash works — a bad one costs speed and never correctness, because every cand |
| [`deflate_start_hash`](#deflate-start-hash) | function | — |
| [`deflate_remember`](#deflate-remember) | function | — |
| [`deflate_into`](#deflate-into) | function | A raw deflate stream — no zlib header, no trailing checksum — which is exactly what a ZIP entry of method 8 holds. `zlib |

## Types
{: #types}

### `Deflater`
{: #deflater}

```burxt
class Deflater
```

The window is 32 KB because RFC 1951 fixes it there: a distance is at most 32,768, so a match further back cannot be expressed. `head` is indexed by a hash of three bytes and holds the most recent position with that hash; `prev` chains backwards from any position to the one before it with the same hash. That is the standard structure and it is what makes matching linear-ish rather than quadratic.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/deflate.bx#L61)

## Functions
{: #functions}

### `deflate_put_bits`
{: #deflate-put-bits}

```burxt
function deflate_put_bits(mutable state: Deflater, mutable out: [Int], value: Int, count: Int) -> Int
```

`count` bits of `value`, least significant first. Every field RFC 1951 calls a "data element" goes through here: the block header, and every extra-bits field of a length or a distance.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/deflate.bx#L76)

### `deflate_put_code`
{: #deflate-put-code}

```burxt
function deflate_put_code(mutable state: Deflater, mutable out: [Int], code: Int, count: Int) -> Int
```

A Huffman code, most significant bit first — the opposite order, per §3.1.1. Written a bit at a time rather than by reversing the code, because a reversal helper is a second place for the bit order to be wrong and this loop is obviously the order it says it is.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/deflate.bx#L96)

### `deflate_flush`
{: #deflate-flush}

```burxt
function deflate_flush(mutable state: Deflater, mutable out: [Int]) -> Int
```

Pad the final byte with zeroes. A deflate stream ends on a bit boundary that is rarely a byte boundary, and the padding is not part of the stream — a reader stops at the end-of-block symbol.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/deflate.bx#L108)

### `deflate_put_symbol`
{: #deflate-put-symbol}

```burxt
function deflate_put_symbol(mutable state: Deflater, mutable out: [Int], symbol: Int) -> Int
```

RFC 1951 §3.2.6, written out rather than derived. The four ranges are the specification's, and the codes are the canonical ones its table gives:

```burxt
 0..143    8 bits   00110000 .. 10111111    (0x30 + symbol)
 144..255  9 bits   110010000 .. 111111111  (0x190 + symbol - 144)
 256..279  7 bits   0000000 .. 0010111      (symbol - 256)
 280..287  8 bits   11000000 .. 11000111    (0xC0 + symbol - 280)
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/deflate.bx#L126)

### `deflate_fill_tables`
{: #deflate-fill-tables}

```burxt
function deflate_fill_tables(mutable state: Deflater) -> Int
```

Filled by a method rather than declared at the top level, because a top-level `let` is not visible inside a function body and a `const` holds a scalar. That is a real constraint of the language and the shape it forces — one filler, called once — is fine.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/deflate.bx#L144)

### `deflate_put_length`
{: #deflate-put-length}

```burxt
function deflate_put_length(mutable state: Deflater, mutable out: [Int], length: Int) -> Int
```

A length becomes the largest code whose base does not exceed it, plus the difference in extra bits. Walked from the top so the first match is the right one.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/deflate.bx#L173)

### `deflate_put_distance`
{: #deflate-put-distance}

```burxt
function deflate_put_distance(mutable state: Deflater, mutable out: [Int], distance: Int) -> Int
```

A distance's code comes from the same shape of table, but its code goes through the fixed DISTANCE tree, which is five bits flat for every code 0..29 — §3.2.6 again, and the one place the two trees differ in a way that is easy to miss.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/deflate.bx#L191)

### `deflate_hash`
{: #deflate-hash}

```burxt
pure function deflate_hash(data: [Int], at: Int) -> Int
```

Three bytes into one of 32,768 buckets. Any hash works — a bad one costs speed and never correctness, because every candidate is verified byte by byte before it is used.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/deflate.bx#L210)

### `deflate_start_hash`
{: #deflate-start-hash}

```burxt
function deflate_start_hash(mutable state: Deflater, count: Int) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/deflate.bx#L217)

### `deflate_remember`
{: #deflate-remember}

```burxt
function deflate_remember(mutable state: Deflater, data: [Int], at: Int) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/deflate.bx#L231)

### `deflate_into`
{: #deflate-into}

```burxt
function deflate_into(mutable out: [Int], data: [Int]) -> Int
```

A raw deflate stream — no zlib header, no trailing checksum — which is exactly what a ZIP entry of method 8 holds. `zlib.decompress(stream, -15)` reads one of these; a zlib stream would need two header bytes and an adler32 after it, which `lib/zip.bx` deliberately does not want.

**One block for the whole input, BFINAL set immediately.** Multiple blocks exist to let a compressor change strategy partway through a stream it is receiving live; nothing here is streaming, so a second block would be a second thing to get wrong for no gain.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/deflate.bx#L247)


{% endraw %}

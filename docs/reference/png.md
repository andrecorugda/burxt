---
layout: doc
title: lib/png.bx
section: reference
description: "PNG decode and encode, in Burxt."
---

{% raw %}

# `lib/png.bx`

PNG decode and encode, in Burxt.

```burxt
use "lib/png.bx";
```

**Extracted from `scripts/editor-icons.bx` on 2026-08-21, and the reason is a test that could not be written.** The decoder implements all five row filters that RFC 2083 defines, and the icon deriver's ten source images between them use four: None 392 rows, Sub 200, Up 1148, Paeth 324, **Average 0**. So one predictor's reconstruction had never executed, in a decoder that reads every PNG this project ships — and no fixture could reach it, because the code lived in a script rather than a module.

The star-burxt session measured the same ratio across twenty-nine real PNGs: **2 Average rows out of 12,913**. That is worse than zero, because it passes and reads as coverage. Neither number can be fixed by finding better artwork, which is the whole argument for `tests/pass/png_row_filters.bx` building its own inputs — one predictor per image, so each filter's path is the entire thing under test.

`Image` lives here rather than in the script because the decoder answers one and the encoder takes one; a caller that wants pixels wants this type.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Image`](#image) | class | thing the Python compared. |
| [`png_be32`](#png-be32) | function | — |
| [`png_push_be32`](#png-push-be32) | function | — |
| [`png_chunk_is`](#png-chunk-is) | function | — |
| [`png_chunk_name`](#png-chunk-name) | function | A chunk's four type bytes, for a diagnostic. Anything outside printable ASCII becomes `?`, because the one file that nee |
| [`png_channels_for`](#png-channels-for) | function | How many samples a colour type carries per pixel, or 0 for one this file will not read. Colour type 3 is a palette and 0 |
| [`png_paeth`](#png-paeth) | function | PNG's fifth filter, from RFC 2083 §6.6. The predictor is whichever of left, above and corner is nearest to `left + above |
| [`png_decode`](#png-decode) | function | Every IDAT payload, inflated once, unfiltered, and widened to RGBA. |
| [`png_chunk`](#png-chunk) | function | One chunk: its length, its type, its data, and a CRC-32 over the type AND the data — over the type as well, which is the |
| [`png_filtered_rows`](#png-filtered-rows) | function | A valid RGBA PNG. **Not Pillow's bytes** — this filters every row with 0 and compresses with fixed Huffman, so the file  |
| [`png_encode_filtered`](#png-encode-filtered) | function | A PNG with every row filtered by one chosen predictor. `png_encode` is this with `None`. |
| [`png_encode`](#png-encode) | function | — |
| [`png_wrap`](#png-wrap) | function | The container around already-filtered scanlines: zlib, IHDR, IDAT, IEND. |

## Types
{: #types}

### `Image`
{: #image}

```burxt
class Image
```

thing the Python compared.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L31)

## Functions
{: #functions}

### `png_be32`
{: #png-be32}

```burxt
pure function png_be32(bytes: [Int], at: Int) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L42)

### `png_push_be32`
{: #png-push-be32}

```burxt
function png_push_be32(mutable out: [Int], value: Int) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L49)

### `png_chunk_is`
{: #png-chunk-is}

```burxt
pure function png_chunk_is(bytes: [Int], at: Int, a: Int, b: Int, c: Int, d: Int) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L58)

### `png_chunk_name`
{: #png-chunk-name}

```burxt
pure function png_chunk_name(bytes: [Int], at: Int) -> String
```

A chunk's four type bytes, for a diagnostic. Anything outside printable ASCII becomes `?`, because the one file that needs this message is a corrupt one and its type bytes may be any value at all — including a NUL, which a Burxt String cannot carry through to the reader.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L68)

### `png_channels_for`
{: #png-channels-for}

```burxt
pure function png_channels_for(colour: Int) -> Int
```

How many samples a colour type carries per pixel, or 0 for one this file will not read. Colour type 3 is a palette and 0 is what says so — the caller turns that into the refusal, because the message belongs where the file name is known and this function does not have it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L87)

### `png_paeth`
{: #png-paeth}

```burxt
pure function png_paeth(left: Int, above: Int, corner: Int) -> Int
```

PNG's fifth filter, from RFC 2083 §6.6. The predictor is whichever of left, above and corner is nearest to `left + above - corner`, with the ties resolved in that order — the order is part of the specification, so a `<` where a `<=` belongs decodes most images and corrupts some.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L106)

### `png_decode`
{: #png-decode}

```burxt
function png_decode(bytes: [Int]) -> Result<Image, String>
```

Every IDAT payload, inflated once, unfiltered, and widened to RGBA.

**The payloads are concatenated before anything inflates.** A PNG may split IDAT anywhere, including mid-symbol, so inflating chunk by chunk reports a truncated stream on a file that is perfectly valid. Nine of the nine PNGs in this repository happen to carry a single IDAT, which is precisely why this is written down rather than discovered later.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L126)

### `png_chunk`
{: #png-chunk}

```burxt
function png_chunk(mutable out: [Int], a: Int, b: Int, c: Int, d: Int, data: [Int]) -> Int
```

One chunk: its length, its type, its data, and a CRC-32 over the type AND the data — over the type as well, which is the part a reader of the format description skips.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L306)

### `png_filtered_rows`
{: #png-filtered-rows}

```burxt
pure function png_filtered_rows(image: Image, filtering: Int) -> [Int]
```

A valid RGBA PNG. **Not Pillow's bytes** — this filters every row with 0 and compresses with fixed Huffman, so the file is bigger than Pillow's and decodes to the same pixels. `--check` compares pixels for exactly that reason: matching an encoder is a different and pointless problem. The filtered scanlines an IDAT holds: one filter-type byte per row, then the row's bytes as differences from whatever that predictor names. RFC 2083 §6.

**One implementation of the five predictors, taking the choice as an argument**, rather than a filtering function per encoder. `png_encode` asks for 0 and always did; the reason this is separable is that `tests/pass/png_row_filters.bx` needs an image per predictor and no artwork supplies one — ten source images here use four of the five, and **Average appears zero times**.

**The wrap is `bit_and(diff, 255)` and not `remainder(diff, 256)`, which is the whole comment.** `remainder` rounds toward zero, so a difference of -3 comes out as -3 where the format says 253, and the image then decodes to ALMOST the right thing — the worst kind of wrong. The star-burxt session paid for that one first and said so; `bit_and` is two's complement and lands on 253.

`left`, `above` and `corner` are read from the ORIGINAL raster, which is correct because a decoder reconstructs those same bytes before it needs them — encoder and decoder walk the same values in the same order, which is what makes the round-trip meaningful rather than circular.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L341)

### `png_encode_filtered`
{: #png-encode-filtered}

```burxt
function png_encode_filtered(image: Image, filtering: Int) -> [Int]
```

A PNG with every row filtered by one chosen predictor. `png_encode` is this with `None`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L381)

### `png_encode`
{: #png-encode}

```burxt
function png_encode(image: Image) -> [Int]
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L389)

### `png_wrap`
{: #png-wrap}

```burxt
function png_wrap(image: Image, rows: [Int]) -> [Int]
```

The container around already-filtered scanlines: zlib, IHDR, IDAT, IEND.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/png.bx#L395)


{% endraw %}

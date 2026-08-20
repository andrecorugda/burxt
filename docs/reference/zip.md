---
layout: doc
title: lib/zip.bx
section: reference
description: "Write a ZIP archive."
---

{% raw %}

# `lib/zip.bx`

Write a ZIP archive.

```burxt
use "lib/zip.bx";
```

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`ZipEntry`](#zipentry) | class | One file in the archive. `bytes` rather than `String` because an archive holds whatever it holds — an icon, a font, a co |
| [`zip_entry`](#zip-entry) | function | — |
| [`zip_entry_text`](#zip-entry-text) | function | The common case: a file whose contents are text. |
| [`zip_le16`](#zip-le16) | function | **These fill the caller's array rather than returning one, and the compiler is why.** The first version returned `[Int]` |
| [`zip_le32`](#zip-le32) | function | — |
| [`zip_dos_date`](#zip-dos-date) | function | **Every entry carries the same stamp, so packing twice gives identical bytes.** This is not tidiness: a committed archiv |
| [`zip_dos_time`](#zip-dos-time) | function | — |
| [`zip_extend`](#zip-extend) | function | Append every element of `more` onto `into`. `push` one at a time, which is what the language gives; the arrays here are  |
| [`zip_into_method`](#zip-into-method) | function | The archive, appended to an array the CALLER owns — same reason as the encoders above, and the same refusal if it tried  |
| [`zip_into`](#zip-into) | function | Write the archive, answering how many bytes went out — the same shape as `write_bytes`, which is what it ends in. |
| [`zip_into_deflated`](#zip-into-deflated) | function | Method 8 per entry, falling back to stored for anything deflate does not shrink. |
| [`zip_write`](#zip-write) | function | — |
| [`zip_write_deflated`](#zip-write-deflated) | function | **The one to reach for when the archive is downloaded.** Storing this project's extension payload instead of deflating i |

## Types
{: #types}

### `ZipEntry`
{: #zipentry}

```burxt
class ZipEntry
```

One file in the archive. `bytes` rather than `String` because an archive holds whatever it holds — an icon, a font, a compiled thing — and a text-only door would send every caller through `to_bytes` anyway while implying the contents must be text.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L50)

## Functions
{: #functions}

### `zip_entry`
{: #zip-entry}

```burxt
pure function zip_entry(name: String, bytes: [Int]) -> ZipEntry
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L55)

### `zip_entry_text`
{: #zip-entry-text}

```burxt
pure function zip_entry_text(name: String, text: String) -> ZipEntry
```

The common case: a file whose contents are text.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L60)

### `zip_le16`
{: #zip-le16}

```burxt
function zip_le16(mutable into: [Int], value: Int) -> Int
```

**These fill the caller's array rather than returning one, and the compiler is why.** The first version returned `[Int]` and was refused: *"function `zip_le16` cannot return [Int], because its storage lives in a region and would not outlive it. Fill an array the caller owns, or return a scalar summary."* That is the memory model doing its job, and the shape it forced is better than the one it refused — an archive is built by appending, so every intermediate two-element array was a copy nobody wanted.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L78)

### `zip_le32`
{: #zip-le32}

```burxt
function zip_le32(mutable into: [Int], value: Int) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L84)

### `zip_dos_date`
{: #zip-dos-date}

```burxt
pure function zip_dos_date() -> Int
```

**Every entry carries the same stamp, so packing twice gives identical bytes.** This is not tidiness: a committed archive that cannot be reproduced cannot be checked against its source, so a stale one is undetectable and a repack-then-inspect step overwrites the evidence before looking at it. That failure was measured in BMX's packer, where three entries moved on every run.

1980-01-01 is the earliest a DOS date can express: the field is `((year - 1980) << 9) | (month << 5) | day`, so `(0 << 9) | (1 << 5) | 1` is 33, and the time field is zero. A reader shows midnight on that date, which is visibly a placeholder rather than a plausible wrong answer.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L103)

### `zip_dos_time`
{: #zip-dos-time}

```burxt
pure function zip_dos_time() -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L107)

### `zip_extend`
{: #zip-extend}

```burxt
function zip_extend(mutable into: [Int], more: [Int]) -> Int
```

Append every element of `more` onto `into`. `push` one at a time, which is what the language gives; the arrays here are a few hundred kilobytes at most and this is linear.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L115)

### `zip_into_method`
{: #zip-into-method}

```burxt
function zip_into_method(mutable out: [Int], entries: [ZipEntry], compress: Bool) -> Int
```

The archive, appended to an array the CALLER owns — same reason as the encoders above, and the same refusal if it tried to return one. Separate from writing so a caller can hand the bytes to something that is not a file, and so a test can check them without a path. **Two doors, one implementation.** `compress` decides method 8 or method 0, and the per-entry fallback below means the deflated door can never produce a larger archive than the stored one — so `zip_write_deflated` is safe to reach for without knowing what is in the entries.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L130)

### `zip_into`
{: #zip-into}

```burxt
function zip_into(mutable out: [Int], entries: [ZipEntry]) -> Int
```

Write the archive, answering how many bytes went out — the same shape as `write_bytes`, which is what it ends in.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L238)

### `zip_into_deflated`
{: #zip-into-deflated}

```burxt
function zip_into_deflated(mutable out: [Int], entries: [ZipEntry]) -> Int
```

Method 8 per entry, falling back to stored for anything deflate does not shrink.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L243)

### `zip_write`
{: #zip-write}

```burxt
function zip_write(path: String, entries: [ZipEntry]) -> Int touches files
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L247)

### `zip_write_deflated`
{: #zip-write-deflated}

```burxt
function zip_write_deflated(path: String, entries: [ZipEntry]) -> Int touches files
```

**The one to reach for when the archive is downloaded.** Storing this project's extension payload instead of deflating it took one `.vsix` from 39,770 to about 73,088 bytes, which is why `lib/deflate.bx` exists at all.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/zip.bx#L256)


{% endraw %}

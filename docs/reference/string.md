---
layout: doc
title: lib/string.bx
section: reference
description: Strings, beyond the four builtins.
---


# `lib/string.bx`

Strings, beyond the four builtins.

```burxt
use "lib/string.bx";
```

A Burxt String is bytes, and the language gives you `len`, `byte_at`, `substring` and `+`. Everything here is written from those four, so nothing in this file can do something a program could not have done itself — which is the point of a standard library rather than a privileged one.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`string_find`](#string-find) | function | Where `needle` first appears in `text`, or -1. The naive scan, deliberately: a compiler's lexer does not search, and a p |
| [`string_contains`](#string-contains) | function | — |
| [`string_starts_with`](#string-starts-with) | function | — |
| [`string_ends_with`](#string-ends-with) | function | — |
| [`string_trim`](#string-trim) | function | Whitespace removed from both ends: space, tab, newline, carriage return. |
| [`string_is_space`](#string-is-space) | function | — |
| [`string_split`](#string-split) | function | Split on a separator STRING. Answers a growable array, so the pieces are new Strings and they have to live somewhere. |
| [`string_matches_at`](#string-matches-at) | function | Does `needle` sit at `at` in `text`? Compared in place, because `substring` would allocate once per position and a split |
| [`string_lines`](#string-lines) | function | Lines, on either ending. A CRLF file and an LF file split identically, which is the whole reason a multi-character separ |
| [`string_to_int`](#string-to-int) | function | The number, or the fallback you chose. Use this when a default is genuinely right — a missing count is zero, a missing p |
| [`string_parse_int`](#string-parse-int) | function | The number, or nothing — for when there is no sensible default and the program has to say what it does about bad input. |
| [`string_join`](#string-join) | function | The separator between each piece. The join a program would write, written once. |
| [`string_repeat`](#string-repeat) | function | — |
| [`char_count`](#char-count) | function | How many codepoints, not bytes. `char_count("héllo")` is 5 where `len` is 6. |
| [`next_char`](#next-char) | function | The byte offset one past the codepoint that starts at `at` — where the NEXT one begins. |
| [`char_at`](#char-at) | function | The `i`'th codepoint, as a one-codepoint String. There is no char type: see the header. |
| [`is_valid_utf8`](#is-valid-utf8) | function | Is every byte part of a well-formed UTF-8 sequence? |

## Functions
{: #functions}

### `string_find`
{: #string-find}

```burxt
function string_find(text: String, needle: String) -> Int
```

Where `needle` first appears in `text`, or -1. The naive scan, deliberately: a compiler's lexer does not search, and a program that needs Boyer-Moore knows it does. Simple until measured.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L17)

### `string_contains`
{: #string-contains}

```burxt
function string_contains(text: String, needle: String) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L42)

### `string_starts_with`
{: #string-starts-with}

```burxt
function string_starts_with(text: String, prefix: String) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L46)

### `string_ends_with`
{: #string-ends-with}

```burxt
function string_ends_with(text: String, suffix: String) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L60)

### `string_trim`
{: #string-trim}

```burxt
function string_trim(text: String) -> String
```

Whitespace removed from both ends: space, tab, newline, carriage return.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L77)

### `string_is_space`
{: #string-is-space}

```burxt
function string_is_space(byte: Int) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L89)

### `string_split`
{: #string-split}

```burxt
function string_split(text: String, separator: String) -> [String]
```

Split on a separator STRING. Answers a growable array, so the pieces are new Strings and they have to live somewhere.

It took an `Int` until v0.0.188 — `string_split(text, 44)` for a comma — which meant a caller had to know ASCII codes, and `", "` and `"\r\n"` could not be split on at all. That is not an inconvenience, it is ordinary text handling out of reach: CSV with spaces after the commas, and any file written on Windows.

One spelling per concept, so the byte form is gone rather than kept beside this. A separator is a string; that a one-character string is also a byte is not a second idea.

An EMPTY separator answers the whole text as one piece rather than splitting into characters. Burxt has no character type — a String is bytes — so "split into characters" is not a thing this could mean, and looping forever on a zero-width match is the alternative.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L107)

### `string_matches_at`
{: #string-matches-at}

```burxt
function string_matches_at(text: String, needle: String, at: Int) -> Bool
```

Does `needle` sit at `at` in `text`? Compared in place, because `substring` would allocate once per position and a split has no business needing a region.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L133)

### `string_lines`
{: #string-lines}

```burxt
function string_lines(text: String) -> [String]
```

Lines, on either ending. A CRLF file and an LF file split identically, which is the whole reason a multi-character separator had to exist: `"\r\n"` could not be written as a byte.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L150)

### `string_to_int`
{: #string-to-int}

```burxt
function string_to_int(text: String, fallback: Int) -> Int
```

The number, or the fallback you chose. Use this when a default is genuinely right — a missing count is zero, a missing page number is one.

```burxt
 let port: Int = string_to_int(configured, 8080);
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L183)

### `string_parse_int`
{: #string-parse-int}

```burxt
function string_parse_int(text: String) -> Option<Int>
```

The number, or nothing — for when there is no sensible default and the program has to say what it does about bad input.

```burxt
 match string_parse_int(field) {
     None    => { print("not a number: " + field); }
     Some(n) => { print(n * 2); }
 }
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L197)

### `string_join`
{: #string-join}

```burxt
function string_join(pieces: [String], separator: String) -> String
```

The separator between each piece. The join a program would write, written once.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L227)

### `string_repeat`
{: #string-repeat}

```burxt
function string_repeat(text: String, times: Int) -> String
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L240)

### `char_count`
{: #char-count}

```burxt
pure function char_count(text: String) -> Int
```

How many codepoints, not bytes. `char_count("héllo")` is 5 where `len` is 6.

A continuation byte is `10xxxxxx`, so every byte that is not one begins a codepoint and the count is a single pass with no decoding. Assumes valid UTF-8: see the note above.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L314)

### `next_char`
{: #next-char}

```burxt
pure function next_char(text: String, at: Int) -> Int
```

The byte offset one past the codepoint that starts at `at` — where the NEXT one begins.

The width is in the leading byte and nowhere else, which is the property that makes UTF-8 walkable in one direction without a table: `0xxxxxxx` is one byte, `110xxxxx` two, `1110xxxx` three, `11110xxx` four.

Total by construction, and both halves of that matter. It always advances at least one byte, so a `while at < len(text)` loop terminates on any input including bytes that are not UTF-8 at all; and it never answers past `len(text)`, so `substring(text, at, next_char(text, at) - at)` is in range even when the last sequence is truncated. An unexpected byte — a stray continuation, an `0xFF` — advances by one, which resynchronises rather than guessing.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L337)

### `char_at`
{: #char-at}

```burxt
pure function char_at(text: String, i: Int) -> String
```

The `i`'th codepoint, as a one-codepoint String. There is no char type: see the header.

A PRECONDITION rather than an empty String for an index out of range, which is this language's habit and the better answer here — an empty String would be a legal value that silently stands in for a mistake, and a caller comparing it against something would get `false` rather than a refusal. `requires` says the mistake out loud, at the call, naming the value.

The second clause calls `char_count`, which is only possible because a contract clause may call a `pure` function — and that a `pure` FUNCTION may be called from a clause is older than A4. What A4 added is the same for a method, so this composes without needing it.

O(n) in `i`, so a loop counting up through it is O(n²). Use `next_char` to walk.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L371)

### `is_valid_utf8`
{: #is-valid-utf8}

```burxt
pure function is_valid_utf8(text: String) -> Bool
```

Is every byte part of a well-formed UTF-8 sequence?

The full rule, not the structural half. A checker that accepted overlong encodings and surrogates would be a blanket that reads like a rule — the shape that let `?` go unimplemented for its whole life — so the second byte's range depends on the leader, which is where the three exclusions live:

* **overlong**: `C0`/`C1` could only encode something a shorter sequence already can, and `E0`

```burxt
 and `F0` restrict the second byte for the same reason. Two spellings of one codepoint is how
 a validator and a decoder come to disagree about a string.
```

* **surrogates**: `ED A0`..`ED BF` is U+D800..U+DFFF, which UTF-8 does not encode. * **out of range**: past `F4 8F` is above U+10FFFF, which is not a codepoint.

Assumes nothing. This is the function `spec/ROADMAP-1.0.md` §B5 would need if the declared-and- unenforced UTF-8 invariant were ever enforced at the four entry points — enforcing it is not done here, and having something to enforce it WITH is the part A5 delivers.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L399)


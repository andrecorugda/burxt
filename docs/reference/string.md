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
| [`is_continuation`](#is-continuation) | function | A continuation byte — `10xxxxxx`, the second and later byte of a multi-byte sequence, and never the first. §D1p asks for |
| [`codepoint_at`](#codepoint-at) | function | The codepoint at codepoint index `i`, as a number. `codepoint_at("é", 0)` is 233. |
| [`to_bytes`](#to-bytes) | function | Every byte, as numbers. §D1p asks for it, and `from_bytes` at the bottom of this file is the inverse — `from_bytes(to_by |
| [`char_index`](#char-index) | function | The CODEPOINT index of `of`, or None. The companion to `string_find`, which answers a BYTE offset. |
| [`string_reverse`](#string-reverse) | function | Built in 4 KB chunks rather than one prepend per character, which is the idiom this project has paid for three times (v0 |
| [`string_to_upper_ascii`](#string-to-upper-ascii) | function | `a`..`z` become `A`..`Z`. Every other byte, ASCII or not, is passed through unchanged. |
| [`string_to_lower_ascii`](#string-to-lower-ascii) | function | `A`..`Z` become `a`..`z`. Every other byte, ASCII or not, is passed through unchanged. |
| [`ascii_letter`](#ascii-letter) | function | The one-byte String for an ASCII LETTER code. |
| [`is_ascii`](#is-ascii) | function | Is every byte below 128? Equivalently: is this text unchanged by any of the byte-wise functions above, and safe to treat |
| [`all_digits`](#all-digits) | function | Is every byte `0`..`9`? **Not a number check** — no sign, no digit grouping, no bounds. It says what its name says, and  |
| [`is_alpha`](#is-alpha) | function | Is every byte an ASCII letter? Same ASCII-only limit: `é` is not alphabetic to this function, and a full answer needs th |
| [`from_codepoint`](#from-codepoint) | function | A codepoint as its UTF-8 bytes. `from_codepoint(233)` is `"é"`, and it is the exact inverse of `codepoint_at` — which is |
| [`from_bytes`](#from-bytes) | function | Bytes back into a String, the exact inverse of `to_bytes`. `from_bytes(to_bytes(s)) == s` for every String, including on |

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

An EMPTY separator answers the whole text as one piece rather than splitting into characters.

~~Burxt has no character type — a String is bytes — so "split into characters" is not a thing this could mean.~~ **That reason expired with A5 (v0.0.250).** A one-codepoint String is exactly what `char_at` answers, so "split into characters" now means something precise. The BEHAVIOUR here is unchanged and the decision is deliberately still open; what follows is the argument, not a ruling.

The case for leaving it: an empty separator has **no forced answer**. It occurs between every pair of characters, and also at the start, at the end, and unboundedly often at each position — so "cut at every occurrence" does not determine a result. Python refuses `"abc".split("")` outright; JavaScript answers UTF-16 code units and cuts emoji in half. Neither is a spelling worth copying.

And when there is more than one right answer, this project's habit is to make the caller pick by NAME — `divide_floor` against `divide_toward_zero`, `shift_right_zeros` against `shift_right_sign`, `string_to_upper_ascii` against a full mapping that does not exist yet. By that habit the codepoint split is a separate `string_chars(text) -> [String]`, six lines over `next_char`, and not a degenerate case of this one. It is not written, because which way this goes is not a call to make inside a comment.

The one thing that would be wrong is silence: looping forever on a zero-width match.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L124)

### `string_matches_at`
{: #string-matches-at}

```burxt
function string_matches_at(text: String, needle: String, at: Int) -> Bool
```

Does `needle` sit at `at` in `text`? Compared in place, because `substring` would allocate once per position and a split has no business needing a region.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L150)

### `string_lines`
{: #string-lines}

```burxt
function string_lines(text: String) -> [String]
```

Lines, on either ending. A CRLF file and an LF file split identically, which is the whole reason a multi-character separator had to exist: `"\r\n"` could not be written as a byte.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L167)

### `string_to_int`
{: #string-to-int}

```burxt
function string_to_int(text: String, fallback: Int) -> Int
```

The number, or the fallback you chose. Use this when a default is genuinely right — a missing count is zero, a missing page number is one.

```burxt
 let port: Int = string_to_int(configured, 8080);
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L200)

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

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L214)

### `string_join`
{: #string-join}

```burxt
function string_join(pieces: [String], separator: String) -> String
```

The separator between each piece. The join a program would write, written once.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L244)

### `string_repeat`
{: #string-repeat}

```burxt
function string_repeat(text: String, times: Int) -> String
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L257)

### `char_count`
{: #char-count}

```burxt
pure function char_count(text: String) -> Int
```

How many codepoints, not bytes. `char_count("héllo")` is 5 where `len` is 6.

A continuation byte is `10xxxxxx`, so every byte that is not one begins a codepoint and the count is a single pass with no decoding. Assumes valid UTF-8: see the note above.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L331)

### `next_char`
{: #next-char}

```burxt
pure function next_char(text: String, at: Int) -> Int
```

The byte offset one past the codepoint that starts at `at` — where the NEXT one begins.

The width is in the leading byte and nowhere else, which is the property that makes UTF-8 walkable in one direction without a table: `0xxxxxxx` is one byte, `110xxxxx` two, `1110xxxx` three, `11110xxx` four.

Total by construction, and both halves of that matter. It always advances at least one byte, so a `while at < len(text)` loop terminates on any input including bytes that are not UTF-8 at all; and it never answers past `len(text)`, so `substring(text, at, next_char(text, at) - at)` is in range even when the last sequence is truncated. An unexpected byte — a stray continuation, an `0xFF` — advances by one, which resynchronises rather than guessing.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L354)

### `char_at`
{: #char-at}

```burxt
pure function char_at(text: String, i: Int) -> String
```

The `i`'th codepoint, as a one-codepoint String. There is no char type: see the header.

A PRECONDITION rather than an empty String for an index out of range, which is this language's habit and the better answer here — an empty String would be a legal value that silently stands in for a mistake, and a caller comparing it against something would get `false` rather than a refusal. `requires` says the mistake out loud, at the call, naming the value.

The second clause calls `char_count`, which is only possible because a contract clause may call a `pure` function — and that a `pure` FUNCTION may be called from a clause is older than A4. What A4 added is the same for a method, so this composes without needing it.

O(n) in `i`, so a loop counting up through it is O(n²). Use `next_char` to walk.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L388)

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

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L416)

### `is_continuation`
{: #is-continuation}

```burxt
pure function is_continuation(byte: Int) -> Bool
```

A continuation byte — `10xxxxxx`, the second and later byte of a multi-byte sequence, and never the first. §D1p asks for it by name, and it is one line, but it is the line every other function in this section is built on: `char_count` counts bytes that are NOT this, and `next_char` reads a width only from bytes that are NOT this.

Takes a BYTE, not a String, and so is spelled like `string_is_space` — the two are the same kind of question asked of the same kind of value.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L489)

### `codepoint_at`
{: #codepoint-at}

```burxt
pure function codepoint_at(text: String, i: Int) -> Int
```

The codepoint at codepoint index `i`, as a number. `codepoint_at("é", 0)` is 233.

The companion to `char_at`, which answers the same character as a one-codepoint String. Two functions rather than one because the two answers are used for different things: a String goes into output and a comparison, a number goes into a range test and arithmetic. Neither is derivable from the other without the decode below, so having only one would make every caller write it.

The width comes from `next_char` rather than from the leading byte a second time, which is what makes this total: `next_char` clamps to `len(text)`, so no read below can go past the end even on a truncated final sequence. It ASSUMES valid UTF-8 for its ANSWER — a truncated three-byte sequence decodes as though it were the two bytes that are there, which is a well-defined number and not the codepoint anyone meant. Same standing as `char_count` and `char_at`; see the section header.

O(n) in `i` for the same reason `char_at` is. Walk with `next_char` if you want all of them.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L509)

### `to_bytes`
{: #to-bytes}

```burxt
function to_bytes(text: String) -> [Int]
```

Every byte, as numbers. §D1p asks for it, and `from_bytes` at the bottom of this file is the inverse — `from_bytes(to_bytes(s)) == s` for every String. This note used to say the partner did not exist and that getting bytes back into a String was "the thing this language cannot do yet"; the `byte_as_string` builtin (§A13) is what changed, in v0.0.259.

`len` and `byte_at` already give a caller this one byte at a time, so this exists for the case where the bytes have to be held: passed to a function, sorted, hashed, compared as a whole.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L550)

### `char_index`
{: #char-index}

```burxt
function char_index(text: String, of: String) -> Option<Int>
```

The CODEPOINT index of `of`, or None. The companion to `string_find`, which answers a BYTE offset.

**Both exist because they answer different questions, and mixing them corrupts text.** A byte offset is what `substring` takes; a codepoint index is what `char_at` takes and what a human counting characters means. In `"héllo"` the `l` is at byte 3 and character 2, and handing 3 to `char_at` reads the wrong character while handing 2 to `substring` splits the `é` in half. The names are the only thing standing between a caller and that bug, which is why neither is called `index_of`.

Matches only on a CODEPOINT BOUNDARY, and that is a real difference from `string_find` rather than an implementation detail: searching `"é"` for the single byte `0xA9` succeeds byte-wise and has no codepoint index to answer, so this walks boundaries and compares from each one. A needle that only occurs straddling a character is reported as absent, which is the truthful answer to the question actually asked.

An EMPTY needle answers `Some(0)`, matching `string_find`'s 0 — every string contains the empty string, at the beginning.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L575)

### `string_reverse`
{: #string-reverse}

```burxt
pure function string_reverse(text: String) -> String allocates
```

Built in 4 KB chunks rather than one prepend per character, which is the idiom this project has paid for three times (v0.0.68, v0.0.77, v0.0.82) and `lib/os.bx` uses for the same reason: a String is immutable, so `out = piece + out` copies everything already collected on every character and turns a reverse into O(n²). The chunk bounds that copy to 4 KB.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L622)

### `string_to_upper_ascii`
{: #string-to-upper-ascii}

```burxt
pure function string_to_upper_ascii(text: String) -> String allocates
```

`a`..`z` become `A`..`Z`. Every other byte, ASCII or not, is passed through unchanged.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L677)

### `string_to_lower_ascii`
{: #string-to-lower-ascii}

```burxt
pure function string_to_lower_ascii(text: String) -> String allocates
```

`A`..`Z` become `a`..`z`. Every other byte, ASCII or not, is passed through unchanged.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L696)

### `ascii_letter`
{: #ascii-letter}

```burxt
pure function ascii_letter(code: Int) -> String
```

The one-byte String for an ASCII LETTER code.

**Two 26-character tables and a `substring` until v0.0.259**, because there was no way to build a String from a byte value in this language and `substring` of a literal was the only Int-to-String path there was. It worked here precisely because the 52 letters were the only bytes the two case functions ever need to produce. `byte_as_string` (§A13) makes the tables unnecessary, and the body is now one line.

**It is kept rather than replaced by the builtin, and the difference is the CONTRACT, not the conversion.** The three `requires` clauses say this is a letter code and nothing else, which is what makes the two callers above readable: `ascii_letter(byte - 32)` states that the arithmetic landed in the letter range, and the compiler checks it. `byte_as_string(byte - 32)` would accept any byte and say nothing. So this is a narrowing, unlike `from_byte`, which would have been a second spelling and is not written.

`requires` rather than a fallback: a code outside the two letter runs is a caller mistake, and answering `"?"` for it is how `os_byte_as_string` came to destroy data silently (roadmap §B2).

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L731)

### `is_ascii`
{: #is-ascii}

```burxt
pure function is_ascii(text: String) -> Bool
```

Is every byte below 128? Equivalently: is this text unchanged by any of the byte-wise functions above, and safe to treat as one byte per character?

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L748)

### `all_digits`
{: #all-digits}

```burxt
pure function all_digits(text: String) -> Bool
```

Is every byte `0`..`9`? **Not a number check** — no sign, no digit grouping, no bounds. It says what its name says, and `string_parse_int` is the one that answers whether the text is an Int. Arabic-Indic and Devanagari digits are NOT digits here, which follows from every byte being ASCII and is the same limit `_ascii` names elsewhere.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L761)

### `is_alpha`
{: #is-alpha}

```burxt
pure function is_alpha(text: String) -> Bool
```

Is every byte an ASCII letter? Same ASCII-only limit: `é` is not alphabetic to this function, and a full answer needs the Unicode category tables that full case mapping needs.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L773)

### `from_codepoint`
{: #from-codepoint}

```burxt
pure function from_codepoint(code: Int) -> String allocates
```

A codepoint as its UTF-8 bytes. `from_codepoint(233)` is `"é"`, and it is the exact inverse of `codepoint_at` — which is the property the fixture checks, over every codepoint in every one of the four widths rather than a chosen few.

**Four branches, mirroring `codepoint_at`'s decode.** The leading byte carries the width marker and the top bits; each continuation carries six more, marked `10xxxxxx`. Written as the inverse of the decoder directly above it so the pair can be read together — a mask there is a shift here.

`requires` rather than a fallback, and this is the same call `ascii_letter` records: a number that is not a codepoint is a caller mistake, and answering U+FFFD for it would be the silent substitution `os_byte_as_string`'s `"?"` was. The two exclusions are what UTF-8 cannot encode:

* **above U+10FFFF** there is no codepoint, so there is no encoding to produce. * **U+D800..U+DFFF are SURROGATES** — half of a UTF-16 pair, never a character on their own.

```burxt
 Encoding one produces CESU-8, which `is_valid_utf8` rejects, so allowing it here would let
 this function build text its own module calls invalid.
```

**It never emits a PARTIAL sequence**, which is the whole reason to prefer it over the builtin: every branch appends all of its bytes or the contract refuses before any of them. A caller assembling text one codepoint at a time therefore cannot produce a truncated character.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L833)

### `from_bytes`
{: #from-bytes}

```burxt
pure function from_bytes(xs: [Int]) -> String allocates
```

Bytes back into a String, the exact inverse of `to_bytes`. `from_bytes(to_bytes(s)) == s` for every String, including one holding a NUL — checked in the fixture, because a length-prefixed String makes that ordinary and it would be easy to assume otherwise.

**It does NOT promise valid UTF-8**, and that is not a gap to fill later: the bytes are the caller's, and a function that validated them would have to answer something for the invalid case, which is either a lie or an `Option` that every caller with known-good bytes then has to unwrap. `is_valid_utf8(from_bytes(xs))` says it in one line where a reader can see it. This is also the only way to build a deliberately BINARY String — which `write_bytes` then writes out.

Chunked, not `out += b`, for the reason three earlier functions in this file carry: appending to a String copies it, so byte-at-a-time is O(n²). The project has paid for that three times.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L868)


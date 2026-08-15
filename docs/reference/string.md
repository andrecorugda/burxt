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
| [`string_join_chunks`](#string-join-chunks) | function | A list of pieces into the one String they spell, by repeated PAIRWISE merge. |
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
| [`string_repeat`](#string-repeat) | function | `text` repeated `times` over. `string_repeat("ab", 3)` is `"ababab"`. |
| [`char_count`](#char-count) | function | How many codepoints, not bytes. `char_count("héllo")` is 5 where `len` is 6. |
| [`next_char`](#next-char) | function | The byte offset one past the codepoint that starts at `at` — where the NEXT one begins. |
| [`char_at`](#char-at) | function | The `i`'th codepoint, as a one-codepoint String. There is no char type: see the header. |
| [`is_valid_utf8`](#is-valid-utf8) | function | Is every byte part of a well-formed UTF-8 sequence? |
| [`is_continuation`](#is-continuation) | function | A continuation byte — `10xxxxxx`, the second and later byte of a multi-byte sequence, and never the first. §D1p asks for |
| [`codepoint_at`](#codepoint-at) | function | The codepoint at codepoint index `i`, as a number. `codepoint_at("é", 0)` is 233. |
| [`to_bytes`](#to-bytes) | function | Every byte, as numbers. §D1p asks for it, and `from_bytes` at the bottom of this file is the inverse — `from_bytes(to_by |
| [`char_index`](#char-index) | function | The CODEPOINT index of `of`, or None. The companion to `string_find`, which answers a BYTE offset. |
| [`string_reverse`](#string-reverse) | function | The §D0 chunk list, and this function is where the difference between it and the 4 KB two-level version it replaced is e |
| [`string_to_upper_ascii`](#string-to-upper-ascii) | function | `a`..`z` become `A`..`Z`. Every other byte, ASCII or not, is passed through unchanged. |
| [`string_to_lower_ascii`](#string-to-lower-ascii) | function | `A`..`Z` become `a`..`z`. Every other byte, ASCII or not, is passed through unchanged. |
| [`ascii_letter`](#ascii-letter) | function | The one-byte String for an ASCII LETTER code. |
| [`is_ascii`](#is-ascii) | function | Is every byte below 128? Equivalently: is this text unchanged by any of the byte-wise functions above, and safe to treat |
| [`all_digits`](#all-digits) | function | Is every byte `0`..`9`? **Not a number check** — no sign, no digit grouping, no bounds. It says what its name says, and  |
| [`is_alpha`](#is-alpha) | function | Is every byte an ASCII letter? Same ASCII-only limit: `é` is not alphabetic to this function, and a full answer needs th |
| [`from_codepoint`](#from-codepoint) | function | A codepoint as its UTF-8 bytes. `from_codepoint(233)` is `"é"`, and it is the exact inverse of `codepoint_at` — which is |
| [`from_bytes`](#from-bytes) | function | Bytes back into a String, the exact inverse of `to_bytes`. `from_bytes(to_bytes(s)) == s` for every String, including on |
| [`string_fold_ascii`](#string-fold-ascii) | function | One byte, case-folded for comparison: `A`..`Z` become `a`..`z` and every other byte, ASCII or not, is itself. |
| [`string_equals_ignore_case`](#string-equals-ignore-case) | function | Same text, ASCII case ignored. Allocates nothing; see the section note. |
| [`string_compare`](#string-compare) | function | -1, 0 or 1: is `a` before, equal to, or after `b`? |
| [`string_compare_ignore_case`](#string-compare-ignore-case) | function | `string_compare`, with ASCII case folded away. Allocates nothing. |
| [`string_find_ignore_case`](#string-find-ignore-case) | function | `string_find`, with ASCII case folded away. A BYTE offset, like `string_find`, and -1 for absent. Allocates nothing — th |
| [`string_capitalise`](#string-capitalise) | function | The empty String answers itself. A first character that is multi-byte is unchanged, because `_ascii` case mapping touche |
| [`string_title_case`](#string-title-case) | function | A word STARTS at a byte that is word-ish and follows one that is not. Word-ish means ASCII alphanumeric **or any byte >= |
| [`string_replace`](#string-replace) | function | Every occurrence of `from` becomes `to`. The bytes between matches are copied as whole RUNS, so a replace that matches r |
| [`string_replace_first`](#string-replace-first) | function | The FIRST occurrence of `from` becomes `to`; the rest of the text is untouched. |
| [`string_pad_start`](#string-pad-start) | function | `fill` repeated on the LEFT until the text is `width` codepoints wide. Already wide enough, or wider, answers the text u |
| [`string_pad_end`](#string-pad-end) | function | `fill` repeated on the RIGHT until the text is `width` codepoints wide. Same contract as `string_pad_start`. |
| [`string_trim_start`](#string-trim-start) | function | Whitespace removed from the START only — space, tab, newline, carriage return, the same four `string_trim` uses. One `su |
| [`string_trim_end`](#string-trim-end) | function | Whitespace removed from the END only. |
| [`string_strip_prefix`](#string-strip-prefix) | function | The text without `prefix`, or **the text unchanged** if it does not start with it. |
| [`string_strip_suffix`](#string-strip-suffix) | function | The text without `suffix`, or the text unchanged. Same call as `string_strip_prefix`. |
| [`string_find_from`](#string-find-from) | function | Where `needle` first appears at or after `from`, or -1. `from` below 0 is read as 0. |
| [`string_rfind`](#string-rfind) | function | Where `needle` LAST appears, or -1. Scanned from the right, so it stops at the first hit rather than finding every occur |
| [`string_count`](#string-count) | function | How many NON-OVERLAPPING occurrences of `needle`. `string_count("aaaa", "aa")` is 2, not 3. |
| [`string_split_space`](#string-split-space) | function | Split on RUNS of whitespace, dropping empty pieces. What `split` on a single space cannot do. |
| [`string_split_once`](#string-split-once) | function | The text cut at the FIRST separator, as a `(before, after)` pair — or None if the separator is not there. |
| [`string_rsplit`](#string-rsplit) | function | `string_split`, scanned from the RIGHT, pieces answered last-first. |
| [`string_split_no_empty`](#string-split-no-empty) | function | `string_split`, with the empty pieces dropped. |
| [`string_is_digit`](#string-is-digit) | function | `0`..`9`. |
| [`string_is_alpha`](#string-is-alpha) | function | `A`..`Z` or `a`..`z`. ASCII only: no byte of a multi-byte character is a letter to this. |
| [`string_is_alnum`](#string-is-alnum) | function | A letter or a digit, ASCII. |
| [`string_is_upper`](#string-is-upper) | function | `A`..`Z`. |
| [`string_is_lower`](#string-is-lower) | function | `a`..`z`. |
| [`string_is_hex_digit`](#string-is-hex-digit) | function | `0`..`9`, `a`..`f` or `A`..`F`. |
| [`string_digit_value`](#string-digit-value) | function | The value of a digit byte in base 36: `0`..`9` is 0..9, `a`/`A` is 10, `z`/`Z` is 35, and **anything else is -1**. |
| [`string_is_blank`](#string-is-blank) | function | Empty, or nothing but whitespace. The check a program reads a config file with, and the reason it is not `string_trim(li |
| [`all_alnum`](#all-alnum) | function | Is every byte an ASCII letter or digit? Named for `all_digits`, not for `string_is_alnum`. |
| [`all_hex_digits`](#all-hex-digits) | function | Is every byte `0`..`9`, `a`..`f` or `A`..`F`? Named for `all_digits` for the same reason. |
| [`string_parse_int_base`](#string-parse-int-base) | function | A number in any base from 2 to 36, or None. Digits are `0`..`9` then `a`..`z`, case-insensitive. |
| [`string_parse_hex`](#string-parse-hex) | function | Hexadecimal, with an optional `0x` or `0X` prefix. `string_parse_hex("0xff")` and `string_parse_hex("FF")` are both 255. |
| [`string_int_to_base`](#string-int-to-base) | function | `out = digit + out` is the quadratic shape §D0 exists to refuse, and it is correct here for a reason that has to be stat |
| [`string_int_to_hex`](#string-int-to-hex) | function | Base 16, lower case, no `0x`. `string_int_to_hex(255)` is `"ff"`. |
| [`string_int_to_binary`](#string-int-to-binary) | function | Base 2, no `0b`. `string_int_to_binary(5)` is `"101"`. |
| [`string_int_padded`](#string-int-padded) | function | An Int zero-padded to at least `width` characters. `string_int_padded(7, 3)` is `"007"`. |

## Functions
{: #functions}

### `string_join_chunks`
{: #string-join-chunks}

```burxt
pure function string_join_chunks(chunks: [String]) -> String
```

A list of pieces into the one String they spell, by repeated PAIRWISE merge.

Public, because it is the other half of the idiom above: a caller writing its own loop needs this to finish, and a private one would mean every caller re-deriving the merge — which is how the left fold gets written by accident.

`log2(n)` passes, each copying every byte exactly once, so `n log n` bytes moved in total against the flat fold's `n²`. The `while len(joined) > 1` shape is `join_chunks` in `src/burxt-compiler/emit.bx`, which is the reference implementation and was measured first.

It does not mutate its argument: the first pass READS `chunks` and writes a fresh `merged`, and `joined` is rebound rather than pushed into. That matters because an array is a handle — pushing into `joined` on the first pass would grow the caller's own list.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L114)

### `string_find`
{: #string-find}

```burxt
pure function string_find(text: String, needle: String) -> Int
```

Where `needle` first appears in `text`, or -1. The naive scan, deliberately: a compiler's lexer does not search, and a program that needs Boyer-Moore knows it does. Simple until measured.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L139)

### `string_contains`
{: #string-contains}

```burxt
pure function string_contains(text: String, needle: String) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L164)

### `string_starts_with`
{: #string-starts-with}

```burxt
pure function string_starts_with(text: String, prefix: String) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L168)

### `string_ends_with`
{: #string-ends-with}

```burxt
pure function string_ends_with(text: String, suffix: String) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L182)

### `string_trim`
{: #string-trim}

```burxt
pure function string_trim(text: String) -> String
```

Whitespace removed from both ends: space, tab, newline, carriage return.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L199)

### `string_is_space`
{: #string-is-space}

```burxt
pure function string_is_space(byte: Int) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L211)

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

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L246)

### `string_matches_at`
{: #string-matches-at}

```burxt
pure function string_matches_at(text: String, needle: String, at: Int) -> Bool
```

Does `needle` sit at `at` in `text`? Compared in place, because `substring` would allocate once per position and a split has no business needing a region.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L272)

### `string_lines`
{: #string-lines}

```burxt
function string_lines(text: String) -> [String]
```

Lines, on either ending. A CRLF file and an LF file split identically, which is the whole reason a multi-character separator had to exist: `"\r\n"` could not be written as a byte.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L289)

### `string_to_int`
{: #string-to-int}

```burxt
function string_to_int(text: String, fallback: Int) -> Int
```

The number, or the fallback you chose. Use this when a default is genuinely right — a missing count is zero, a missing page number is one.

```burxt
 let port: Int = string_to_int(configured, 8080);
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L322)

### `string_parse_int`
{: #string-parse-int}

```burxt
pure function string_parse_int(text: String) -> Option<Int>
```

The number, or nothing — for when there is no sensible default and the program has to say what it does about bad input.

```burxt
 match string_parse_int(field) {
     None    => { print("not a number: " + field); }
     Some(n) => { print(n * 2); }
 }
```

**One line on top of `string_parse_int_base`, since v0.0.279, and that is a bug fix rather than a tidy-up.** The hand-written base-10 accumulator this used to hold was `value = value * 10 + digit` with no overflow guard, and Burxt's `*` traps — so `string_parse_int("99999999999999999999")` did not answer None, it **ended the process** with `arithmetic overflow` and exit code 70. Measured. See the §D1d section header for why that was worse than the silent 0 this function was fixed for in v0.0.152, and for how the guard below it works.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L343)

### `string_join`
{: #string-join}

```burxt
function string_join(pieces: [String], separator: String) -> String
```

The separator between each piece. The join a program would write, written once.

The §D0 chunk list. This was `out += separator + pieces[i]` until the D1 work measured it, which made joining a 100 KB document out of its lines cost the square of its length — in the one function a program reaches for precisely because it has a lot of pieces.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L352)

### `string_repeat`
{: #string-repeat}

```burxt
pure function string_repeat(text: String, times: Int) -> String
```

`text` repeated `times` over. `string_repeat("ab", 3)` is `"ababab"`.

**By DOUBLING, not by a chunk list, and this is the one place in the file where the chunk list is not the right answer.** `out += text` in a loop was the original and is quadratic. A chunk list would fix that and copy `n log(times)` bytes; doubling copies `2n` and needs no list at all, because a repeat is the one builder that already knows every piece is identical. The same binary decomposition `math_pow` uses: `doubled` holds `text` repeated a power-of-two number of times, and the bits of `times` say which powers to keep.

`times <= 0` answers `""` rather than refusing. Zero copies of something is a real and useful answer — it is what `string_pad_start` asks for when the text already fills the width — and a negative count is the same question asked clumsily. This is the case where a fallback is right and a `requires` would make every caller guard a value it does not care about.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L390)

### `char_count`
{: #char-count}

```burxt
pure function char_count(text: String) -> Int
```

How many codepoints, not bytes. `char_count("héllo")` is 5 where `len` is 6.

A continuation byte is `10xxxxxx`, so every byte that is not one begins a codepoint and the count is a single pass with no decoding. Assumes valid UTF-8: see the note above.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L475)

### `next_char`
{: #next-char}

```burxt
pure function next_char(text: String, at: Int) -> Int
```

The byte offset one past the codepoint that starts at `at` — where the NEXT one begins.

The width is in the leading byte and nowhere else, which is the property that makes UTF-8 walkable in one direction without a table: `0xxxxxxx` is one byte, `110xxxxx` two, `1110xxxx` three, `11110xxx` four.

Total by construction, and both halves of that matter. It always advances at least one byte, so a `while at < len(text)` loop terminates on any input including bytes that are not UTF-8 at all; and it never answers past `len(text)`, so `substring(text, at, next_char(text, at) - at)` is in range even when the last sequence is truncated. An unexpected byte — a stray continuation, an `0xFF` — advances by one, which resynchronises rather than guessing.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L498)

### `char_at`
{: #char-at}

```burxt
pure function char_at(text: String, i: Int) -> String
```

The `i`'th codepoint, as a one-codepoint String. There is no char type: see the header.

A PRECONDITION rather than an empty String for an index out of range, which is this language's habit and the better answer here — an empty String would be a legal value that silently stands in for a mistake, and a caller comparing it against something would get `false` rather than a refusal. `requires` says the mistake out loud, at the call, naming the value.

The second clause calls `char_count`, which is only possible because a contract clause may call a `pure` function — and that a `pure` FUNCTION may be called from a clause is older than A4. What A4 added is the same for a method, so this composes without needing it.

O(n) in `i`, so a loop counting up through it is O(n²). Use `next_char` to walk.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L532)

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

Assumes nothing. This is the function `spec/1.0/ROADMAP-1.0.md` §B5 would need if the declared-and- unenforced UTF-8 invariant were ever enforced at the four entry points — enforcing it is not done here, and having something to enforce it WITH is the part A5 delivers.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L560)

### `is_continuation`
{: #is-continuation}

```burxt
pure function is_continuation(byte: Int) -> Bool
```

A continuation byte — `10xxxxxx`, the second and later byte of a multi-byte sequence, and never the first. §D1p asks for it by name, and it is one line, but it is the line every other function in this section is built on: `char_count` counts bytes that are NOT this, and `next_char` reads a width only from bytes that are NOT this.

Takes a BYTE, not a String, and so is spelled like `string_is_space` — the two are the same kind of question asked of the same kind of value.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L633)

### `codepoint_at`
{: #codepoint-at}

```burxt
pure function codepoint_at(text: String, i: Int) -> Int
```

The codepoint at codepoint index `i`, as a number. `codepoint_at("é", 0)` is 233.

The companion to `char_at`, which answers the same character as a one-codepoint String. Two functions rather than one because the two answers are used for different things: a String goes into output and a comparison, a number goes into a range test and arithmetic. Neither is derivable from the other without the decode below, so having only one would make every caller write it.

The width comes from `next_char` rather than from the leading byte a second time, which is what makes this total: `next_char` clamps to `len(text)`, so no read below can go past the end even on a truncated final sequence. It ASSUMES valid UTF-8 for its ANSWER — a truncated three-byte sequence decodes as though it were the two bytes that are there, which is a well-defined number and not the codepoint anyone meant. Same standing as `char_count` and `char_at`; see the section header.

O(n) in `i` for the same reason `char_at` is. Walk with `next_char` if you want all of them.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L653)

### `to_bytes`
{: #to-bytes}

```burxt
function to_bytes(text: String) -> [Int]
```

Every byte, as numbers. §D1p asks for it, and `from_bytes` at the bottom of this file is the inverse — `from_bytes(to_bytes(s)) == s` for every String. This note used to say the partner did not exist and that getting bytes back into a String was "the thing this language cannot do yet"; the `byte_as_string` builtin (§A13) is what changed, in v0.0.259.

`len` and `byte_at` already give a caller this one byte at a time, so this exists for the case where the bytes have to be held: passed to a function, sorted, hashed, compared as a whole.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L694)

### `char_index`
{: #char-index}

```burxt
function char_index(text: String, of: String) -> Option<Int>
```

The CODEPOINT index of `of`, or None. The companion to `string_find`, which answers a BYTE offset.

**Both exist because they answer different questions, and mixing them corrupts text.** A byte offset is what `substring` takes; a codepoint index is what `char_at` takes and what a human counting characters means. In `"héllo"` the `l` is at byte 3 and character 2, and handing 3 to `char_at` reads the wrong character while handing 2 to `substring` splits the `é` in half. The names are the only thing standing between a caller and that bug, which is why neither is called `index_of`.

Matches only on a CODEPOINT BOUNDARY, and that is a real difference from `string_find` rather than an implementation detail: searching `"é"` for the single byte `0xA9` succeeds byte-wise and has no codepoint index to answer, so this walks boundaries and compares from each one. A needle that only occurs straddling a character is reported as absent, which is the truthful answer to the question actually asked.

An EMPTY needle answers `Some(0)`, matching `string_find`'s 0 — every string contains the empty string, at the beginning.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L719)

### `string_reverse`
{: #string-reverse}

```burxt
pure function string_reverse(text: String) -> String
```

The §D0 chunk list, and this function is where the difference between it and the 4 KB two-level version it replaced is easiest to see. A String is immutable, so `out = piece + out` copies everything collected so far on every character and turns a reverse into O(n²); a pending chunk bounds that copy, but flushing it into a flat `done` is quadratic again in the number of flushes. Both levels have to be a list. See the section at the top of this file for the numbers.

**The chunks come out in reverse order and are reversed again to join**, which is the one thing here that is not the plain idiom: each chunk holds its own bytes already reversed, so the whole answer is the LAST chunk first.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L771)

### `string_to_upper_ascii`
{: #string-to-upper-ascii}

```burxt
pure function string_to_upper_ascii(text: String) -> String
```

`a`..`z` become `A`..`Z`. Every other byte, ASCII or not, is passed through unchanged.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L850)

### `string_to_lower_ascii`
{: #string-to-lower-ascii}

```burxt
pure function string_to_lower_ascii(text: String) -> String
```

`A`..`Z` become `a`..`z`. Every other byte, ASCII or not, is passed through unchanged.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L881)

### `ascii_letter`
{: #ascii-letter}

```burxt
pure function ascii_letter(code: Int) -> String
```

The one-byte String for an ASCII LETTER code.

**Two 26-character tables and a `substring` until v0.0.259**, because there was no way to build a String from a byte value in this language and `substring` of a literal was the only Int-to-String path there was. It worked here precisely because the 52 letters were the only bytes the two case functions ever need to produce. `byte_as_string` (§A13) makes the tables unnecessary, and the body is now one line.

**It is kept rather than replaced by the builtin, and the difference is the CONTRACT, not the conversion.** The three `requires` clauses say this is a letter code and nothing else, which is what makes the two callers above readable: `ascii_letter(byte - 32)` states that the arithmetic landed in the letter range, and the compiler checks it. `byte_as_string(byte - 32)` would accept any byte and say nothing. So this is a narrowing, unlike `from_byte`, which would have been a second spelling and is not written.

`requires` rather than a fallback: a code outside the two letter runs is a caller mistake, and answering `"?"` for it is how `os_byte_as_string` came to destroy data silently (roadmap §B2).

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L928)

### `is_ascii`
{: #is-ascii}

```burxt
pure function is_ascii(text: String) -> Bool
```

Is every byte below 128? Equivalently: is this text unchanged by any of the byte-wise functions above, and safe to treat as one byte per character?

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L945)

### `all_digits`
{: #all-digits}

```burxt
pure function all_digits(text: String) -> Bool
```

Is every byte `0`..`9`? **Not a number check** — no sign, no digit grouping, no bounds. It says what its name says, and `string_parse_int` is the one that answers whether the text is an Int. Arabic-Indic and Devanagari digits are NOT digits here, which follows from every byte being ASCII and is the same limit `_ascii` names elsewhere.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L958)

### `is_alpha`
{: #is-alpha}

```burxt
pure function is_alpha(text: String) -> Bool
```

Is every byte an ASCII letter? Same ASCII-only limit: `é` is not alphabetic to this function, and a full answer needs the Unicode category tables that full case mapping needs.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L970)

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

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1030)

### `from_bytes`
{: #from-bytes}

```burxt
pure function from_bytes(xs: [Int]) -> String
```

Bytes back into a String, the exact inverse of `to_bytes`. `from_bytes(to_bytes(s)) == s` for every String, including one holding a NUL — checked in the fixture, because a length-prefixed String makes that ordinary and it would be easy to assume otherwise.

**It does NOT promise valid UTF-8**, and that is not a gap to fill later: the bytes are the caller's, and a function that validated them would have to answer something for the invalid case, which is either a lie or an `Option` that every caller with known-good bytes then has to unwrap. `is_valid_utf8(from_bytes(xs))` says it in one line where a reader can see it. This is also the only way to build a deliberately BINARY String — which `write_bytes` then writes out.

The §D0 chunk list, not `out += b`, for the reason the section at the top of this file measures: appending to a String copies it, so byte-at-a-time is O(n²) and a flat flush target is O(n²) in the flushes. There is no run to copy wholesale here — every byte is a separate `[Int]` element and has to be converted — so this is the plain idiom.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1067)

### `string_fold_ascii`
{: #string-fold-ascii}

```burxt
pure function string_fold_ascii(byte: Int) -> Int
```

One byte, case-folded for comparison: `A`..`Z` become `a`..`z` and every other byte, ASCII or not, is itself.

Takes a BYTE, like `string_is_space`, and is the shared kernel of the three `_ignore_case` functions. Folding to LOWER rather than upper is arbitrary for ASCII — the two agree on every comparison — and is chosen to match what `equals_ignore_case` would do in every other language.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1113)

### `string_equals_ignore_case`
{: #string-equals-ignore-case}

```burxt
pure function string_equals_ignore_case(a: String, b: String) -> Bool
```

Same text, ASCII case ignored. Allocates nothing; see the section note.

**A length check first, and it is a real shortcut rather than an optimisation.** ASCII folding never changes a byte's length, so two Strings that differ only in ASCII case have the same byte length — which means an early `false` on differing lengths cannot be wrong.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1125)

### `string_compare`
{: #string-compare}

```burxt
pure function string_compare(a: String, b: String) -> Int
```

-1, 0 or 1: is `a` before, equal to, or after `b`?

**Byte-wise, and for UTF-8 that IS codepoint order** — a designed property of the encoding rather than a coincidence, and the reason this needs no codepoint walk. A shorter String that is a prefix of a longer one sorts first, which is the ordinary lexicographic rule.

Three-way rather than the `<` the language already has, because a sort or a binary search wants one comparison and not two. It is *not* a second spelling of `<`: `a < b` answers one of the three questions and this answers which of the three it is.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1149)

### `string_compare_ignore_case`
{: #string-compare-ignore-case}

```burxt
pure function string_compare_ignore_case(a: String, b: String) -> Int
```

`string_compare`, with ASCII case folded away. Allocates nothing.

Note what this is NOT: a case-insensitive ORDER for non-ASCII text. `"Ä"` and `"ä"` compare unequal here and always will without the Unicode tables. The `_ascii` limit again.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1181)

### `string_find_ignore_case`
{: #string-find-ignore-case}

```burxt
pure function string_find_ignore_case(text: String, needle: String) -> Int
```

`string_find`, with ASCII case folded away. A BYTE offset, like `string_find`, and -1 for absent. Allocates nothing — the point of it existing beside `string_find` rather than callers lowering both sides first.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1212)

### `string_capitalise`
{: #string-capitalise}

```burxt
pure function string_capitalise(text: String) -> String
```

The empty String answers itself. A first character that is multi-byte is unchanged, because `_ascii` case mapping touches no byte >= 128 — and this splits the text at BYTE 1, so a two-byte `é` is briefly two invalid halves that concatenate back to exactly what they were. Nothing inspects the halves, and the answer is correct; the fixture checks it on `"élan"`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1258)

### `string_title_case`
{: #string-title-case}

```burxt
pure function string_title_case(text: String) -> String
```

A word STARTS at a byte that is word-ish and follows one that is not. Word-ish means ASCII alphanumeric **or any byte >= 128**, and that second clause is the case a naive version gets wrong rather than a detail:

* Every byte of a multi-byte UTF-8 character is >= 128. Treating those as non-word-ish would

```burxt
 make the byte AFTER a `é` a word start — so `"élan"` would come out `"éLan"`. Measured on
 exactly that input before the clause was added.
```

* So `é` is word-ish here even though this function cannot case-map it. That is the honest

```burxt
 ASCII-limited answer: it does not claim to know the character's case, only that it is part
 of a word and does not end one.
```

Punctuation and digits therefore behave like this, all of which the fixture pins:

```burxt
 "o'brien"      ->  "O'Brien"      an apostrophe ends a word, as it does in Python
 "3rd place"    ->  "3rd Place"    a digit is word-ish, so `r` is not a word start
 "hello-world"  ->  "Hello-World"
```

The apostrophe case is the one people disagree about. It is recorded rather than argued: a rule that made `'` word-ish would give `"O'brien"`, and neither is wrong for every name.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1294)

### `string_replace`
{: #string-replace}

```burxt
pure function string_replace(text: String, from: String, to: String) -> String
```

Every occurrence of `from` becomes `to`. The bytes between matches are copied as whole RUNS, so a replace that matches rarely costs about one pass whatever the text's length.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1349)

### `string_replace_first`
{: #string-replace-first}

```burxt
pure function string_replace_first(text: String, from: String, to: String) -> String
```

The FIRST occurrence of `from` becomes `to`; the rest of the text is untouched.

Not `string_replace` with a counter, because it needs no builder at all: one `find` and two substrings, which is three allocations regardless of how long the text is.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1390)

### `string_pad_start`
{: #string-pad-start}

```burxt
pure function string_pad_start(text: String, width: Int, fill: String) -> String
```

`fill` repeated on the LEFT until the text is `width` codepoints wide. Already wide enough, or wider, answers the text unchanged — never truncates.

**`fill` must be exactly one codepoint**, as a `requires` rather than a silent truncation: a two-character fill cannot in general reach a given width at all (`"ab"` can pad to an even number of columns and no odd one), so there is no correct answer to give and refusing at the call is the only honest option.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1423)

### `string_pad_end`
{: #string-pad-end}

```burxt
pure function string_pad_end(text: String, width: Int, fill: String) -> String
```

`fill` repeated on the RIGHT until the text is `width` codepoints wide. Same contract as `string_pad_start`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1435)

### `string_trim_start`
{: #string-trim-start}

```burxt
pure function string_trim_start(text: String) -> String
```

Whitespace removed from the START only — space, tab, newline, carriage return, the same four `string_trim` uses. One `substring`, so it allocates once whatever it removes.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1447)

### `string_trim_end`
{: #string-trim-end}

```burxt
pure function string_trim_end(text: String) -> String
```

Whitespace removed from the END only.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1457)

### `string_strip_prefix`
{: #string-strip-prefix}

```burxt
pure function string_strip_prefix(text: String, prefix: String) -> String
```

The text without `prefix`, or **the text unchanged** if it does not start with it.

Rust answers an `Option` here and this does not, and the reason is that the question "did it have the prefix?" already has a function: `string_starts_with`. An `Option` would make every caller who knows the answer write a `match` to get back to the String, and the two-function form lets a caller who cares ask and a caller who does not just strip. `string_split_once` answers an `Option` because there the "absent" case has no sensible String to return.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1472)

### `string_strip_suffix`
{: #string-strip-suffix}

```burxt
pure function string_strip_suffix(text: String, suffix: String) -> String
```

The text without `suffix`, or the text unchanged. Same call as `string_strip_prefix`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1481)

### `string_find_from`
{: #string-find-from}

```burxt
pure function string_find_from(text: String, needle: String, from: Int) -> Int
```

Where `needle` first appears at or after `from`, or -1. `from` below 0 is read as 0.

This is what a loop over every occurrence needs, and without it a caller writes `string_find(substring(text, from, ...), needle) + from` — which allocates a copy of the rest of the text on every iteration and makes the loop quadratic. That is the whole reason it exists.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1516)

### `string_rfind`
{: #string-rfind}

```burxt
pure function string_rfind(text: String, needle: String) -> Int
```

Where `needle` LAST appears, or -1. Scanned from the right, so it stops at the first hit rather than finding every occurrence and keeping the last.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1540)

### `string_count`
{: #string-count}

```burxt
pure function string_count(text: String, needle: String) -> Int
```

How many NON-OVERLAPPING occurrences of `needle`. `string_count("aaaa", "aa")` is 2, not 3.

Non-overlapping because that is what `string_replace` does, and a `count` that disagreed with the `replace` beside it would be a trap: a caller sizing something by `count` and then calling `replace` would get a different number of substitutions than it was told.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1561)

### `string_split_space`
{: #string-split-space}

```burxt
function string_split_space(text: String) -> [String]
```

Split on RUNS of whitespace, dropping empty pieces. What `split` on a single space cannot do.

`string_split(" a  b ", " ")` answers five pieces, three of them empty. This answers `["a", "b"]`, which is what every program reading a table, a log line or a command line actually wants — and the difference is not a convenience: a caller using `string_split` on space has to filter the empties itself, and the version that forgets is a program that breaks on a double space.

Leading and trailing whitespace therefore produce NO empty piece, and `""` and `"   "` both answer an empty list. This is Python's `.split()` with no argument, and Rust's `split_whitespace` — the one shape everybody agrees on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1590)

### `string_split_once`
{: #string-split-once}

```burxt
function string_split_once(text: String, separator: String) -> Option<(String, String)>
```

The text cut at the FIRST separator, as a `(before, after)` pair — or None if the separator is not there.

**An `Option`, unlike `string_strip_prefix`, and the difference is that there is no sensible pair to answer when the separator is absent.** `("key", "")` would be a lie about a line with no `=` in it, and it is exactly the lie that turns a malformed config file into a program with an empty password. This is the shape a `key=value` reader wants:

```burxt
 match string_split_once(line, "=") {
     None       => { print_error("no = in: " + line); }
     Some(pair) => { set(pair.0, pair.1); }
 }
```

The separator itself is in NEITHER half. An empty separator answers None, agreeing with the rest of the file.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1624)

### `string_rsplit`
{: #string-rsplit}

```burxt
function string_rsplit(text: String, separator: String) -> [String]
```

`string_split`, scanned from the RIGHT, pieces answered last-first.

**It is NOT `string_split`'s answer reversed, and that is measured rather than assumed.** For a separator that can overlap itself, which occurrences match depends on which end you start from:

```burxt
 string_split("aaa", "aa")            ->  ["", "a"]   matched at 0; "a" is the TAIL
 that list, reversed                  ->  ["a", ""]
 string_rsplit("aaa", "aa")           ->  ["", "a"]   matched at 1; "a" is the HEAD
```

So the two lists differ — `["a", ""]` against `["", "a"]` — and even where they print alike the pieces mean opposite things: the empty one is the tail of a left scan and the head of a right one. The fixture prints all three lists side by side rather than leaving that to this comment.

For an ordinary non-overlapping separator this IS `string_split` read backwards, which is the common case and the reason to have it: the last field of a dotted name is `string_rsplit(name, ".")[0]`, with no need to know how many fields there were.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1654)

### `string_split_no_empty`
{: #string-split-no-empty}

```burxt
function string_split_no_empty(text: String, separator: String) -> [String]
```

`string_split`, with the empty pieces dropped.

The difference matters most at the ends. `string_split("a,,b,", ",")` answers `["a", "", "b", ""]` — four fields, which is what a CSV reader must see — and this answers `["a", "b"]`, which is what a reader of a `PATH`-style list wants. Two functions because the two callers need different things and neither can be derived from the other cheaply by hand: a caller who filters `string_split` themselves and forgets is a program that breaks on `"a,,b"`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1685)

### `string_is_digit`
{: #string-is-digit}

```burxt
pure function string_is_digit(byte: Int) -> Bool
```

`0`..`9`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1719)

### `string_is_alpha`
{: #string-is-alpha}

```burxt
pure function string_is_alpha(byte: Int) -> Bool
```

`A`..`Z` or `a`..`z`. ASCII only: no byte of a multi-byte character is a letter to this.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1724)

### `string_is_alnum`
{: #string-is-alnum}

```burxt
pure function string_is_alnum(byte: Int) -> Bool
```

A letter or a digit, ASCII.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1729)

### `string_is_upper`
{: #string-is-upper}

```burxt
pure function string_is_upper(byte: Int) -> Bool
```

`A`..`Z`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1734)

### `string_is_lower`
{: #string-is-lower}

```burxt
pure function string_is_lower(byte: Int) -> Bool
```

`a`..`z`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1739)

### `string_is_hex_digit`
{: #string-is-hex-digit}

```burxt
pure function string_is_hex_digit(byte: Int) -> Bool
```

`0`..`9`, `a`..`f` or `A`..`F`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1744)

### `string_digit_value`
{: #string-digit-value}

```burxt
pure function string_digit_value(byte: Int) -> Int
```

The value of a digit byte in base 36: `0`..`9` is 0..9, `a`/`A` is 10, `z`/`Z` is 35, and **anything else is -1**.

-1 rather than an `Option`, and this is the one place in the file where that is the right call: it is the inner loop of every parse below, an `Option` per byte would allocate nothing but would cost a `match` per digit, and the caller's very next line is a range check against the base anyway — `digit < 0 || digit >= base` catches "not a digit" and "not a digit IN THIS BASE" in one comparison. The sentinel is safe here because -1 is outside every base's digit range, so a caller who forgets the check gets a refusal from the range test rather than a wrong number.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1759)

### `string_is_blank`
{: #string-is-blank}

```burxt
pure function string_is_blank(text: String) -> Bool
```

Empty, or nothing but whitespace. The check a program reads a config file with, and the reason it is not `string_trim(line) == ""`: that builds a String to answer a Bool, and on a large file it does it once per line.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1775)

### `all_alnum`
{: #all-alnum}

```burxt
pure function all_alnum(text: String) -> Bool
```

Is every byte an ASCII letter or digit? Named for `all_digits`, not for `string_is_alnum`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1788)

### `all_hex_digits`
{: #all-hex-digits}

```burxt
pure function all_hex_digits(text: String) -> Bool
```

Is every byte `0`..`9`, `a`..`f` or `A`..`F`? Named for `all_digits` for the same reason.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1801)

### `string_parse_int_base`
{: #string-parse-int-base}

```burxt
pure function string_parse_int_base(text: String, base: Int) -> Option<Int>
```

A number in any base from 2 to 36, or None. Digits are `0`..`9` then `a`..`z`, case-insensitive.

An optional leading `-` or `+`. **Accepting `+` is a change from what `string_parse_int` used to do**, and it is deliberate: C, Rust and Python all accept it, and the alternative was one parser in this file that took `"+5"` and another that did not.

Refuses, rather than traps or truncates: an empty String, a sign with no digits after it, a byte that is not a digit in this base, and anything whose value does not fit an Int. There is no whitespace tolerance — `string_trim` is one call and doing it silently would make `" 1 2"` a question this function has to have an opinion about.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1854)

### `string_parse_hex`
{: #string-parse-hex}

```burxt
pure function string_parse_hex(text: String) -> Option<Int>
```

Hexadecimal, with an optional `0x` or `0X` prefix. `string_parse_hex("0xff")` and `string_parse_hex("FF")` are both 255.

**The prefix is the only reason this exists** rather than being `string_parse_int_base(s, 16)` under a second name — which the project's one-spelling-per-concept rule would refuse. A sign goes before the prefix: `"-0x10"` is -16, and `"0x-10"` is not a number.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1915)

### `string_int_to_base`
{: #string-int-to-base}

```burxt
pure function string_int_to_base(value: Int, base: Int) -> String
```

`out = digit + out` is the quadratic shape §D0 exists to refuse, and it is correct here for a reason that has to be stated or it will be "fixed" into a chunk list by someone reading the rule and not the bound: **the digit count is at most 64**, at base 2, for any Int. So the total copied is at most 64*65/2 = 2,080 bytes, whatever the value. It is a constant, not a growth rate.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1958)

### `string_int_to_hex`
{: #string-int-to-hex}

```burxt
pure function string_int_to_hex(value: Int) -> String
```

Base 16, lower case, no `0x`. `string_int_to_hex(255)` is `"ff"`.

A named base rather than a second spelling of `string_int_to_base`: the base is the argument a reader cannot check by eye, and `string_int_to_hex(x)` cannot be miswritten as base 61. The same argument Rust's `{:x}` makes against `{:radix$}`. Upper case is `string_to_upper_ascii` of this.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1986)

### `string_int_to_binary`
{: #string-int-to-binary}

```burxt
pure function string_int_to_binary(value: Int) -> String
```

Base 2, no `0b`. `string_int_to_binary(5)` is `"101"`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L1991)

### `string_int_padded`
{: #string-int-padded}

```burxt
pure function string_int_padded(value: Int, width: Int) -> String
```

An Int zero-padded to at least `width` characters. `string_int_padded(7, 3)` is `"007"`.

**The sign goes before the zeros and counts toward the width**, so `string_int_padded(-7, 4)` is `"-007"` and not `"0-07"` or `"-0007"`. That is printf's `%04d` and every other language's answer; the alternatives are respectively unreadable and off by one.

A number already that wide is unchanged — never truncated. Padding that silently dropped a digit would be a wrong number in a column that looks right, which is the failure this language exists to refuse.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L2004)


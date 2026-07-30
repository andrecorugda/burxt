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
| [`string_split`](#string-split) | function | Split on a single byte. Answers a growable array, so it needs a region — the pieces are new Strings and they have to liv |
| [`string_lines`](#string-lines) | function | — |
| [`string_to_int`](#string-to-int) | function | The number, or the fallback you chose. Use this when a default is genuinely right — a missing count is zero, a missing p |
| [`string_parse_int`](#string-parse-int) | function | The number, or nothing — for when there is no sensible default and the program has to say what it does about bad input. |
| [`string_join`](#string-join) | function | The separator between each piece. The join a program would write, written once. |
| [`string_repeat`](#string-repeat) | function | — |

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
function string_split(text: String, separator: Int) -> [String]
```

Split on a single byte. Answers a growable array, so it needs a region — the pieces are new Strings and they have to live somewhere.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L95)

### `string_lines`
{: #string-lines}

```burxt
function string_lines(text: String) -> [String]
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L109)

### `string_to_int`
{: #string-to-int}

```burxt
function string_to_int(text: String, fallback: Int) -> Int
```

The number, or the fallback you chose. Use this when a default is genuinely right — a missing count is zero, a missing page number is one.

```burxt
 let port: Int = string_to_int(configured, 8080);
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L129)

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

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L143)

### `string_join`
{: #string-join}

```burxt
function string_join(pieces: [String], separator: String) -> String
```

The separator between each piece. The join a program would write, written once.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L173)

### `string_repeat`
{: #string-repeat}

```burxt
function string_repeat(text: String, times: Int) -> String
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/string.bx#L186)


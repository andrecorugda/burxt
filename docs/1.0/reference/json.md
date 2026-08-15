---
layout: doc
title: lib/json.bx
section: reference
description: JSON, parsed and rendered, in ordinary Burxt.
---

> **This documents Burxt 1.0.0, and it is frozen.** It will not change again — that is what makes
> it useful to someone still on 1.0. The current documentation is at
> [the top of the site]({{ site.baseurl }}/).



# `lib/json.bx`

JSON, parsed and rendered, in ordinary Burxt.

```burxt
use "lib/json.bx";
```

Nothing here needs a compiler feature. It is the `enum` + `class` mutual recursion from docs/guide/03-types.md and the `Option`/`Result` from lib/, which is the same test `lib/map.bx` passed: if a JSON document had needed a keyword, the type system would not be real.

---- The one position this library takes ------------------------------------------------

**A JSON number is its DIGITS, not a float.**

JSON's own grammar says a number is arbitrary-precision decimal text. Every language then parses it into a `double` on the way in, which is where an exact `19.99` stops being exact — and it is the same abandonment `as scaled` exists to prevent at the C boundary. See spec/1.0/N1-BOUNDARY-EXACTNESS.md: real financial defects live at boundaries, not in arithmetic.

So `Json.Number` holds the digits exactly as they were written, and turning them into a typed value is a separate, checked step that can fail — `json_as_int`, `json_as_money`. Nothing is rounded on the way in, because nothing on the way in knows what rounding you wanted.

And going out, **money crosses as a quoted string**: `json_money($19.99)` renders `"19.99"`, not `19.99`. A JSON number reaches a JavaScript consumer as a double and loses the cent. A string reaches every consumer with all its digits. That costs one `JSON.parse` field being text on the far side, and it is the difference between exact and nearly.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Json`](#json) | enum | A JSON value. |
| [`Field`](#field) | class | One member of an object. A class rather than a two-payload variant, because a name and a value travelling together is a  |
| [`Reader`](#reader) | class | Where the parser is. A class with `mutable self` methods, because Burxt has no writable parameters — the same constraint |
| [`json_null`](#json-null) | function | — |
| [`json_truth`](#json-truth) | function | — |
| [`json_int`](#json-int) | function | — |
| [`json_money`](#json-money) | function | Money, as a quoted string. See the header: this is the position, and it is deliberate. |
| [`json_text`](#json-text) | function | — |
| [`json_list`](#json-list) | function | — |
| [`json_object`](#json-object) | function | — |
| [`json_field`](#json-field) | function | — |
| [`json_escape`](#json-escape) | function | A String with the six escapes JSON requires, and nothing else. |
| [`json_render`](#json-render) | function | One JSON value as text, with no whitespace. Recursive, because the shape is. |
| [`json_at`](#json-at) | function | A member by name, or None. Linear, like every other lookup in this repository at this scale — an MCP request has single- |
| [`json_as_text`](#json-as-text) | function | — |
| [`json_digits`](#json-digits) | function | The digits of a number, whether it arrived as a JSON number or as a quoted string. |
| [`json_as_int`](#json-as-int) | function | — |
| [`json_as_truth`](#json-as-truth) | function | — |
| [`json_as_money`](#json-as-money) | function | Money, at two places, or None when the digits are not exactly that. |
| [`is_json_digit`](#is-json-digit) | function | — |
| [`json_parse`](#json-parse) | function | One JSON document. Trailing whitespace is allowed; trailing anything else is not, because a second document where one wa |
| [`skip_space`](#skip-space) | method on `Reader` | — |
| [`peek`](#peek) | method on `Reader` | — |
| [`parse_value`](#parse-value) | method on `Reader` | Parse one value at the cursor. Recursive through `parse_list` and `parse_object`. |
| [`word`](#word) | method on `Reader` | Consume `word` if it is at the cursor. Answers whether it was. |
| [`parse_number`](#parse-number) | method on `Reader` | A number, kept as the digits it was written with. Only the SHAPE is checked here — that it is a number at all — because  |
| [`parse_unicode_escape`](#parse-unicode-escape) | method on `Reader` | One `\uXXXX`, with `self.at` sitting on the `u`. Answers the character and leaves `self.at` just past the escape. B9. |
| [`read_four_hex`](#read-four-hex) | method on `Reader` | The four hex digits of a `\uXXXX`, with `self.at` on the `u`. Leaves `self.at` past the digits. |
| [`parse_text`](#parse-text) | method on `Reader` | A quoted string, with the escapes undone. |
| [`parse_list`](#parse-list) | method on `Reader` | — |
| [`parse_object`](#parse-object) | method on `Reader` | — |

## Types
{: #types}

### `Json`
{: #json}

```burxt
enum Json
```

A JSON value.

`List` holds a slice of itself and `Object` a slice of `Field`, which holds a `Json` — the two halves of one recursive shape. A slice is a pointer and a length, so neither is infinitely wide, which is why this works where an enum directly inside an enum does not.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L38)

### `Field`
{: #field}

```burxt
class Field { name: String, value: Json }
```

One member of an object. A class rather than a two-payload variant, because a name and a value travelling together is a thing worth naming — and because a variant may not carry an enum, while a class field may.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L52)

### `Reader`
{: #reader}

```burxt
class Reader { text: String, at: Int }
```

Where the parser is. A class with `mutable self` methods, because Burxt has no writable parameters — the same constraint that made `lib/map.bx` method-based, and the same outcome: the cursor being a value you can name reads better than threading an index through twelve returns.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L286)

## Functions
{: #functions}

### `json_null`
{: #json-null}

```burxt
function json_null() -> Json
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L58)

### `json_truth`
{: #json-truth}

```burxt
function json_truth(value: Bool) -> Json
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L62)

### `json_int`
{: #json-int}

```burxt
function json_int(value: Int) -> Json
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L66)

### `json_money`
{: #json-money}

```burxt
function json_money(amount: Decimal<2>) -> Json
```

Money, as a quoted string. See the header: this is the position, and it is deliberate.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L71)

### `json_text`
{: #json-text}

```burxt
function json_text(value: String) -> Json
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L75)

### `json_list`
{: #json-list}

```burxt
function json_list(values: [Json]) -> Json
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L79)

### `json_object`
{: #json-object}

```burxt
function json_object(fields: [Field]) -> Json
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L83)

### `json_field`
{: #json-field}

```burxt
function json_field(name: String, value: Json) -> Field
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L87)

### `json_escape`
{: #json-escape}

```burxt
function json_escape(text: String) -> String
```

A String with the six escapes JSON requires, and nothing else.

Built in RUNS rather than a byte at a time: `out = out + one_byte` copies the whole String on every byte, which this project has paid for three times — the lexer was quadratic for eleven versions on exactly this shape. Everything from `copied` to here is appended in one slice.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L98)

### `json_render`
{: #json-render}

```burxt
function json_render(value: Json) -> String
```

One JSON value as text, with no whitespace. Recursive, because the shape is.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L123)

### `json_at`
{: #json-at}

```burxt
function json_at(value: Json, name: String) -> Option<Json>
```

A member by name, or None. Linear, like every other lookup in this repository at this scale — an MCP request has single-digit field counts, and a map would allocate to save nothing.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L161)

### `json_as_text`
{: #json-as-text}

```burxt
function json_as_text(value: Json) -> Option<String>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L182)

### `json_digits`
{: #json-digits}

```burxt
function json_digits(value: Json) -> Option<String>
```

The digits of a number, whether it arrived as a JSON number or as a quoted string.

Both, on purpose: an exact producer sends money as a string (see the header) and a careless one sends it as a number, and a server that reads only one of the two rejects half its callers for a difference that carries no information.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L198)

### `json_as_int`
{: #json-as-int}

```burxt
function json_as_int(value: Json) -> Option<Int>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L209)

### `json_as_truth`
{: #json-as-truth}

```burxt
function json_as_truth(value: Json) -> Option<Bool>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L217)

### `json_as_money`
{: #json-as-money}

```burxt
function json_as_money(value: Json) -> Option<Decimal<2>>
```

Money, at two places, or None when the digits are not exactly that.

**It never rounds.** `"19.999"` answers None rather than `20.00`, because a value arriving from outside with more precision than you asked for is a question and not a rounding: the caller sent a third decimal place for a reason, and no default here can know what it was. `1.5` is fine — `1.50` loses nothing — and `1.567` is refused.

The reconstruction is a count of pennies times a penny, which is exact by construction and needs no rounding contract, because that is literally what a scaled decimal already is.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L237)

### `is_json_digit`
{: #is-json-digit}

```burxt
function is_json_digit(b: Int) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L389)

### `json_parse`
{: #json-parse}

```burxt
function json_parse(text: String) -> Result<Json, String>
```

One JSON document. Trailing whitespace is allowed; trailing anything else is not, because a second document where one was expected means the caller framed its input wrong and saying so beats parsing half of it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L600)

## Methods
{: #methods}

### `skip_space`
{: #skip-space}

```burxt
function (mutable self: Reader) skip_space() -> Int
```

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L288)

### `peek`
{: #peek}

```burxt
function (self: Reader) peek() -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L295)

### `parse_value`
{: #parse-value}

```burxt
function (mutable self: Reader) parse_value() -> Result<Json, String>
```

Parse one value at the cursor. Recursive through `parse_list` and `parse_object`.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L303)

### `word`
{: #word}

```burxt
function (mutable self: Reader) word(word: String) -> Bool
```

Consume `word` if it is at the cursor. Answers whether it was.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L331)

### `parse_number`
{: #parse-number}

```burxt
function (mutable self: Reader) parse_number() -> Result<Json, String>
```

A number, kept as the digits it was written with. Only the SHAPE is checked here — that it is a number at all — because deciding what type it should become is the caller's, and doing it here would be the silent conversion this file exists to avoid.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L350)

### `parse_unicode_escape`
{: #parse-unicode-escape}

```burxt
function (mutable self: Reader) parse_unicode_escape() -> Result<String, String>
```

One `\uXXXX`, with `self.at` sitting on the `u`. Answers the character and leaves `self.at` just past the escape. B9.

**Surrogate pairs are the reason this is a function rather than four lines inline.** JSON is specified in terms of UTF-16, so every codepoint above U+FFFF is written as TWO escapes — an emoji is `😀`, never one escape. A decoder that treats each `\uXXXX` independently produces two half-characters, and `from_codepoint` refuses those outright (they are surrogates, and encoding one is CESU-8, which this library's own `is_valid_utf8` rejects). So the choice was never "handle pairs or ignore them" — it was "handle pairs or refuse every emoji in real-world JSON".

A lone surrogate, high or low, is an error rather than U+FFFD. Substituting a replacement character is the same silent repair as the `"?"` that `os_byte_as_string` used to make: the caller asked for text and would get text, subtly not the text that was sent.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L407)

### `read_four_hex`
{: #read-four-hex}

```burxt
function (mutable self: Reader) read_four_hex() -> Result<Int, String>
```

The four hex digits of a `\uXXXX`, with `self.at` on the `u`. Leaves `self.at` past the digits.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L445)

### `parse_text`
{: #parse-text}

```burxt
function (mutable self: Reader) parse_text() -> Result<String, String>
```

A quoted string, with the escapes undone.

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L468)

### `parse_list`
{: #parse-list}

```burxt
function (mutable self: Reader) parse_list() -> Result<Json, String>
```

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L520)

### `parse_object`
{: #parse-object}

```burxt
function (mutable self: Reader) parse_object() -> Result<Json, String>
```

Takes `mutable self`, so it changes the value it is called on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/json.bx#L551)


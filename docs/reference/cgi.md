---
layout: doc
title: lib/cgi.bx
section: reference
description: "The request in, the response out, over the interface every web server has."
---


# `lib/cgi.bx`

The request in, the response out, over the interface every web server has.

```burxt
use "lib/cgi.bx";
```

spoken since 1993.

CGI is not a lesser deployment model. The request arrives in environment variables and on stdin, the response leaves on stdout, and the web server owns the crowd — so a Burxt binary behind nginx serves dynamic pages **with no listener, no sockets and no concurrency**. This is how PHP started, and it outlived most of its successors.

---- The one position this library takes ------------------------------------------------

**Malformed input is refused, never repaired.**

Every decoder here answers `Option` and returns `None` on input no correct client would have sent: a `%` with fewer than two hex digits after it, a `%` followed by something that is not hex, a parameter with no `=`. The alternative is what most form parsers do — drop the bad pair, or decode `%zz` as the literal text `%zz` — and both of those are a silent wrong answer in the layer where user input first meets a program. A page that refuses is a page whose author finds out.

The one deliberate exception is a **completely empty** query or body, which is not malformed: it is a request with no parameters, and it answers an empty list.

---- What this owns, and what it does not ------------------------------------------------

It owns percent-decoding, `&`/`=` splitting, and the shape of a response. It does NOT own routing, sessions, cookies, multipart uploads or content negotiation — per M15 §0, Burxt ships primitives and someone else ships the framework.

---- One limit, stated rather than discovered ---------------------------------------------

**A response body is TEXT, and it always ends with a newline.** `print` is the only way out of a Burxt program and it appends one, so `cgi_respond` counts that byte in Content-Length rather than pretending it is not there. For HTML, JSON and CSV that trailing newline is invisible. For a PNG it is corruption — so do not serve binary through this file until there is a way to write bytes without a newline, and that is a compiler gap, not a library one.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Request`](#request) | class | What the server told us, and nothing inferred. `body` is read eagerly because a CGI program that does not drain stdin ca |
| [`Param`](#param) | class | One decoded parameter. A class rather than a tuple, for the reason `Field` is one at lib/json.bx:49 — a name and a value |
| [`cgi_request`](#cgi-request) | function | The request, read from the environment the server set up. |
| [`cgi_hex_value`](#cgi-hex-value) | function | One hex digit's value, or -1. Upper and lower case both, because clients send both. |
| [`cgi_decode`](#cgi-decode) | function | Percent-decoding, in RUNS rather than a byte at a time — the shape recorded at lib/json.bx:98, because `out = out + one_ |
| [`cgi_decode_path`](#cgi-decode-path) | function | A path segment. `+` is an ordinary character in a path — only a form encoding gives it a second meaning, and reading it  |
| [`cgi_decode_form`](#cgi-decode-form) | function | A query or form value, where `+` means space per application/x-www-form-urlencoded. |
| [`cgi_params`](#cgi-params) | function | `a=1&b=two` decoded into pairs, or `None` if any of it is malformed. |
| [`cgi_param`](#cgi-param) | function | The FIRST parameter of that name, or `None`. |
| [`cgi_status_text`](#cgi-status-text) | function | The reason phrase for a status, so a caller writes `200` and not `"200 OK"`. |
| [`cgi_respond`](#cgi-respond) | function | Headers and body on stdout, in CGI's shape: `Status:` rather than an HTTP status line, because the server writes the lin |
| [`cgi_respond_html`](#cgi-respond-html) | function | The pairing this file exists for: a typed tree goes out as a page, and the escaping happened where `lib/html.bx` says it |

## Types
{: #types}

### `Request`
{: #request}

```burxt
class Request
```

What the server told us, and nothing inferred. `body` is read eagerly because a CGI program that does not drain stdin can wedge the server that spawned it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/cgi.bx#L50)

### `Param`
{: #param}

```burxt
class Param { name: String, value: String }
```

One decoded parameter. A class rather than a tuple, for the reason `Field` is one at lib/json.bx:49 — a name and a value travelling together is a thing worth naming.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/cgi.bx#L59)

## Functions
{: #functions}

### `cgi_request`
{: #cgi-request}

```burxt
function cgi_request() -> Request touches input
```

The request, read from the environment the server set up.

A missing variable becomes `""` rather than `None`: CGI guarantees `REQUEST_METHOD`, and for the rest an absent `QUERY_STRING` and an empty one mean the same thing to every caller. That is the opposite call from `os_env`, and it is made here because the two facts are genuinely not different at this layer.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/cgi.bx#L67)

### `cgi_hex_value`
{: #cgi-hex-value}

```burxt
pure function cgi_hex_value(b: Int) -> Int
```

One hex digit's value, or -1. Upper and lower case both, because clients send both.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/cgi.bx#L83)

### `cgi_decode`
{: #cgi-decode}

```burxt
function cgi_decode(text: String, plus_is_space: Bool) -> Option<String>
```

Percent-decoding, in RUNS rather than a byte at a time — the shape recorded at lib/json.bx:98, because `out = out + one_byte` copies the whole String on every byte.

`plus_is_space` is a parameter and not two functions because the two callers below name which they meant, and the rule itself is one line different.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/cgi.bx#L95)

### `cgi_decode_path`
{: #cgi-decode-path}

```burxt
function cgi_decode_path(text: String) -> Option<String>
```

A path segment. `+` is an ordinary character in a path — only a form encoding gives it a second meaning, and reading it as a space here would silently rename a file.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/cgi.bx#L126)

### `cgi_decode_form`
{: #cgi-decode-form}

```burxt
function cgi_decode_form(text: String) -> Option<String>
```

A query or form value, where `+` means space per application/x-www-form-urlencoded.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/cgi.bx#L131)

### `cgi_params`
{: #cgi-params}

```burxt
function cgi_params(encoded: String) -> Option<[Param]>
```

`a=1&b=two` decoded into pairs, or `None` if any of it is malformed.

An empty input is an empty list, not a refusal — see the header. A trailing or doubled `&` yields an empty piece, which is skipped for the same reason: `a=1&` is what a form with one field actually sends.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/cgi.bx#L142)

### `cgi_param`
{: #cgi-param}

```burxt
pure function cgi_param(params: [Param], name: String) -> Option<String>
```

The FIRST parameter of that name, or `None`.

First, not last, and not a list: a repeated name is how the classic parameter-pollution bug works, where the server reads one and a proxy reads the other. First is the rule stated out loud so both sides can agree on it. A caller that genuinely wants every value walks the list `cgi_params` already handed it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/cgi.bx#L177)

### `cgi_status_text`
{: #cgi-status-text}

```burxt
pure function cgi_status_text(status: Int) -> String
```

The reason phrase for a status, so a caller writes `200` and not `"200 OK"`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/cgi.bx#L191)

### `cgi_respond`
{: #cgi-respond}

```burxt
function cgi_respond(status: Int, content_type: String, body: String) -> Int
```

Headers and body on stdout, in CGI's shape: `Status:` rather than an HTTP status line, because the server writes the line and we tell it what to put on it.

The blank line between headers and body is the whole protocol. It is written here, once, rather than left to each caller to remember.

Answers the number of body bytes sent. `print` carries no effect — there is no `output` in the effect vocabulary, because a program that writes to stdout is what a program IS — so the return value is the only fact worth handing back, and a Bool that is always `true` would not have been one.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/cgi.bx#L219)

### `cgi_respond_html`
{: #cgi-respond-html}

```burxt
function cgi_respond_html(status: Int, page: Html) -> Int
```

The pairing this file exists for: a typed tree goes out as a page, and the escaping happened where `lib/html.bx` says it happens. There is no overload taking a String, on purpose — a String reaching this function would be a page nobody escaped.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/cgi.bx#L237)


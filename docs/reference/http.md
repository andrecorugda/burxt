---
layout: doc
title: lib/http.bx
section: reference
description: "HTTP/1.1 over the sockets `net.bx` already opens."
---

{% raw %}

# `lib/http.bx`

HTTP/1.1 over the sockets `net.bx` already opens.

```burxt
use "lib/http.bx";
```

**Why this is Burxt's and not a framework's.** A request arriving on a socket is untrusted bytes becoming typed values, and that boundary is the one thing this language exists to make checkable: an `HttpRequest` whose fields carry contracts is a fact `burxt review` can diff between versions. Put the parser in a framework and the interesting property leaves the type system. It is the same call `lib/html.bx` got when star-burxt moved to its own repository — one escaping implementation, in the place where being right is checkable — and `lib/cgi.bx` is the precedent: HTTP protocol has lived in the standard library since it existed. Routing a request to a page is MEANING, and that belongs to whoever is building the framework.

**Nothing here is new capability.** `net_listen`, `net_accept`, `net_connect_ipv4`, `net_read` and `net_write` have all worked since v1.1.0, and `cgi.bx` already percent-decodes. What was missing was the framing in between: a method, a path, headers, a status line. A server written without it takes an opaque String and answers a document, which is a renderer.

---- what this slice does NOT do, named rather than discovered ---------------------------------

* **No chunked transfer encoding.** A body is read by `Content-Length` or not at all. A request

```burxt
 with `Transfer-Encoding: chunked` is REFUSED by name rather than half-read, because a body
 silently truncated is the wrong answer this language exists to avoid.
```

* **No DNS**, so a client takes four octets. `getaddrinfo` already returns 0 from a Burxt

```burxt
 program; what is missing is one builtin to read a pointer back out of C memory. See
 `docs/limitations.md`.
```

* **No TLS here.** `lib/tls.bx` wraps that, and an https client goes through it. * **No keep-alive.** One request, one response, one connection. `Connection: close` is sent.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`HttpHeader`](#httpheader) | class | Prefixed by hand, like every other name in this library. `cgi.bx` already has a `Request` and `use` concatenates sources |
| [`HttpRequest`](#httprequest) | class | A request, parsed. `target` is what arrived on the wire; `path` and `query` are it split, with the path percent-decoded  |
| [`HttpResponse`](#httpresponse) | class | — |
| [`Handler`](#handler) | interface | What a server does with a request. The interface IS the handler, because Burxt has no function values — and `dynamic Han |
| [`http_parse_request`](#http-parse-request) | function | The wire format, or a refusal that says which part was wrong. |
| [`http_header`](#http-header) | function | A header by name, case-insensitively, because the wire does not agree on capitalisation and a lookup that did would find |
| [`http_content_length`](#http-content-length) | function | How many bytes of body the headers say to expect. `None` when the header is absent or is not a number — the caller decid |
| [`http_query_params`](#http-query-params) | function | The query string, decoded into name/value pairs. Uses `cgi.bx`'s decoder rather than a second one: two percent-decoders  |
| [`http_form_params`](#http-form-params) | function | A form-encoded body, decoded the same way. |
| [`http_response`](#http-response) | function | — |
| [`http_html`](#http-html) | function | — |
| [`http_text`](#http-text) | function | — |
| [`http_json`](#http-json) | function | — |
| [`http_not_found`](#http-not-found) | function | — |
| [`http_with_header`](#http-with-header) | function | One more header on a response. Answers a NEW response rather than mutating, so a handler can build one from another with |
| [`http_render_response`](#http-render-response) | function | The bytes to put on the socket. |
| [`http_parse_response`](#http-parse-response) | function | A response read back from a server, for the client half. |
| [`http_response_header`](#http-response-header) | function | — |
| [`http_read_request`](#http-read-request) | function | Read one request off an accepted connection. |
| [`http_send_response`](#http-send-response) | function | — |
| [`http_serve_one`](#http-serve-one) | function | Serve one connection: read, hand to the handler, answer, close. Answers whether a response was written, so a caller's lo |
| [`http_serve`](#http-serve) | function | Listen, then serve until something stops answering. `None` when the port could not be taken — which is the one failure a |
| [`http_send_request`](#http-send-request) | function | One request to a server at a known address, and the response. |
| [`http_get`](#http-get) | function | The common case. |
| [`http_post`](#http-post) | function | A POST with a body and a content type. |

## Types
{: #types}

### `HttpHeader`
{: #httpheader}

```burxt
class HttpHeader { name: String, value: String }
```

Prefixed by hand, like every other name in this library. `cgi.bx` already has a `Request` and `use` concatenates sources into one buffer, so there is nothing to be namespaced apart — see `docs/limitations.md` on the absence of a registry and of namespacing. `Param` is deliberately NOT redefined here: it comes from `cgi.bx` and one decoded name/value pair should be one type.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L37)

### `HttpRequest`
{: #httprequest}

```burxt
class HttpRequest
```

A request, parsed. `target` is what arrived on the wire; `path` and `query` are it split, with the path percent-decoded and the query left encoded because `http_query` is what decodes it — decoding before splitting is how a `%3D` in a value becomes an extra `=`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L42)

### `HttpResponse`
{: #httpresponse}

```burxt
class HttpResponse
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L52)

### `Handler`
{: #handler}

```burxt
interface Handler
```

What a server does with a request. The interface IS the handler, because Burxt has no function values — and `dynamic Handler` is one in all but name, which is the recorded decision rather than a workaround (`docs/limitations.md`, "No closures"). `lib/fn.bx`'s `Mapper` and `Predicate` are the precedent.

A caller binds its handler to a variable before passing it: an interface object borrows the storage it refers to, so a literal has nowhere to be borrowed from, and the compiler says so.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L299)

## Functions
{: #functions}

### `http_parse_request`
{: #http-parse-request}

```burxt
pure function http_parse_request(text: String) -> Result<HttpRequest, String> allocates
```

The wire format, or a refusal that says which part was wrong.

Every failure names the part rather than the whole: a reader who gets "no request line" and a reader who gets "no blank line after the headers" have different problems, and one message for both sends one of them to the wrong place.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L65)

### `http_header`
{: #http-header}

```burxt
pure function http_header(request: HttpRequest, name: String) -> Option<String>
```

A header by name, case-insensitively, because the wire does not agree on capitalisation and a lookup that did would find `Content-Length` and miss `content-length`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L145)

### `http_content_length`
{: #http-content-length}

```burxt
pure function http_content_length(request: HttpRequest) -> Option<Int>
```

How many bytes of body the headers say to expect. `None` when the header is absent or is not a number — the caller decides whether that means zero or means refuse.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L158)

### `http_query_params`
{: #http-query-params}

```burxt
pure function http_query_params(request: HttpRequest) -> Option<[Param]> allocates
```

The query string, decoded into name/value pairs. Uses `cgi.bx`'s decoder rather than a second one: two percent-decoders is two chances to disagree about `+`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L167)

### `http_form_params`
{: #http-form-params}

```burxt
pure function http_form_params(request: HttpRequest) -> Option<[Param]> allocates
```

A form-encoded body, decoded the same way.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L172)

### `http_response`
{: #http-response}

```burxt
pure function http_response(status: Int, content_type: String, body: String) -> HttpResponse allocates
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L178)

### `http_html`
{: #http-html}

```burxt
pure function http_html(body: String) -> HttpResponse allocates
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L187)

### `http_text`
{: #http-text}

```burxt
pure function http_text(body: String) -> HttpResponse allocates
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L191)

### `http_json`
{: #http-json}

```burxt
pure function http_json(body: String) -> HttpResponse allocates
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L195)

### `http_not_found`
{: #http-not-found}

```burxt
pure function http_not_found() -> HttpResponse allocates
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L199)

### `http_with_header`
{: #http-with-header}

```burxt
pure function http_with_header(response: HttpResponse, name: String, value: String) -> HttpResponse allocates
```

One more header on a response. Answers a NEW response rather than mutating, so a handler can build one from another without the caller wondering which of the two changed.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L205)

### `http_render_response`
{: #http-render-response}

```burxt
pure function http_render_response(response: HttpResponse) -> String allocates
```

The bytes to put on the socket.

`cgi_status_text` supplies **the code AND the reason** — it answers `"200 OK"`, not `"OK"` — which is right for CGI's `Status:` header and is the whole line here. Writing the code again in front of it produced `HTTP/1.1 200 200 OK`, and it was invisible until the bytes were printed: both halves were correct and the composition was not. Reusing it keeps the two transports from disagreeing about what 404 is called, which is worth the sharp edge as long as it is named.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L223)

### `http_parse_response`
{: #http-parse-response}

```burxt
pure function http_parse_response(text: String) -> Result<HttpResponse, String> allocates
```

A response read back from a server, for the client half.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L237)

### `http_response_header`
{: #http-response-header}

```burxt
pure function http_response_header(response: HttpResponse, name: String) -> Option<String>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L279)

### `http_read_request`
{: #http-read-request}

```burxt
function http_read_request(fd: Int, limit: Int) -> Result<HttpRequest, String>
```

Read one request off an accepted connection.

**Two reads, and the second is what makes a body arrive.** `net_read` answers what one `recv` gave, which for a request with a body is usually the head and some of it. So: read until the blank line is in hand, then keep reading until `Content-Length` bytes of body have arrived. A server that read once and parsed would work on every hand-typed request and truncate every real POST — the class of bug that passes a demo and fails a form.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L310)

### `http_send_response`
{: #http-send-response}

```burxt
function http_send_response(fd: Int, response: HttpResponse) -> Bool allocates touches network
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L369)

### `http_serve_one`
{: #http-serve-one}

```burxt
function http_serve_one(server: Int, handler: dynamic Handler, limit: Int) -> Bool
```

Serve one connection: read, hand to the handler, answer, close. Answers whether a response was written, so a caller's loop can count failures rather than guess.

**A malformed request is answered with 400 rather than dropped.** A client that gets nothing back cannot tell a rejection from a hang, and "the server closed on me" is the least actionable thing a server can say.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L382)

### `http_serve`
{: #http-serve}

```burxt
function http_serve(port: Int, handler: dynamic Handler, limit: Int) -> Option<Int>
```

Listen, then serve until something stops answering. `None` when the port could not be taken — which is the one failure a caller can act on, by choosing another.

**No fork here on purpose.** `os_fork` exists and a pre-forked server is written with it, but how many workers and how they are supervised is an application's decision, not a protocol's. This serves one connection at a time and says so; `http_serve_one` is what a forking loop calls.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L409)

### `http_send_request`
{: #http-send-request}

```burxt
function http_send_request(a: Int, b: Int, c: Int, d: Int, port: Int, host: String,
```

One request to a server at a known address, and the response.

**Four octets rather than a hostname, and that is a language gap rather than a design choice.** `getaddrinfo` already returns 0 from a Burxt program; what is missing is one builtin to read a pointer back out of C memory, so the `addrinfo` chain cannot be walked. `docs/limitations.md` carries the measurement. When that builtin lands this grows a `http_get_host` beside it and this function does not change.

`host` is still needed because HTTP/1.1 requires a `Host:` header and a virtual host answers by it — so the name travels even when the address does not come from it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L441)

### `http_get`
{: #http-get}

```burxt
function http_get(a: Int, b: Int, c: Int, d: Int, port: Int, host: String, path: String)
```

The common case.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L491)

### `http_post`
{: #http-post}

```burxt
function http_post(a: Int, b: Int, c: Int, d: Int, port: Int, host: String, path: String,
```

A POST with a body and a content type.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/http.bx#L498)


{% endraw %}

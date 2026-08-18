---
layout: doc
title: lib/tls.bx
section: reference
description: "TLS by BINDING OpenSSL, which is the recorded decision rather than a shortcut."
---

{% raw %}

# `lib/tls.bx`

TLS by BINDING OpenSSL, which is the recorded decision rather than a shortcut.

```burxt
use "lib/tls.bx";
```

`docs/limitations.md` says why Burxt does not implement TLS: this language gives no control over instruction timing or cache behaviour, and a hand-rolled handshake that *looks* fine is exactly the silent wrong answer it exists to refuse. So the primitives are bound, never written, and the same rule sends AES, ChaCha20, RSA and the curves the same way.

**Nothing here needed a compiler change.** A TLS 1.3 handshake worked from a Burxt program with six `external function` declarations before this file existed; what was missing was a module, and the page said "No TLS" for two weeks after v1.0.0 shipped because nobody had tried.

---- the security posture, and every part of it was MEASURED ------------------------------------

**A handshake that succeeds proves nothing on its own**, and this file exists to make that impossible to get wrong by accident. Three things have to be true together:

1. `SSL_CTX_set_default_verify_paths` — the system CAs. Without them the chain cannot be built:

```burxt
  measured, `SSL_connect` answers -1 and the verify result is **20**, no local issuer.
```

2. `SSL_set_verify(ssl, SSL_VERIFY_PEER, NULL)` — **without this the chain is never checked at

```burxt
  all, and `SSL_get_verify_result` answers OK vacuously.** That is the trap: a program can set
  a hostname, read a verify result of 0, and have verified nothing.
```

3. `SSL_set1_host` — the hostname, checked against the certificate. Measured against a name no

```burxt
  certificate could cover: `SSL_connect` answers -1 and the verify result is **62**, hostname
  mismatch.
```

**And the control that nearly fooled me is worth the paragraph.** My first wrong-hostname test used `wrong.example.com` against 1.1.1.1 and verified CLEAN — which read as "verification is not working". It was working: `SSL_get0_peername` reported the matched name as `*.example.com`, which that certificate really does carry. The control was testing a case where the defect could not appear. A name genuinely outside the certificate is what makes the check falsifiable.

---- what this does not do -----------------------------------------------------------------

* **No DNS**, so a caller passes four octets and the hostname separately. The hostname is not

```burxt
 decoration: it is the SNI sent and the name verified, so passing the wrong one is refused.
```

* **No client certificates**, no session resumption, no ALPN — so no HTTP/2. * **Blocking**, like `net.bx` beneath it.

Link with `-lssl -lcrypto`. That is a build-time fact this file cannot state for you.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`TlsLink`](#tlslink) | class | An open, VERIFIED connection. There is no way to make one that is not verified: `tls_connect` is the only constructor an |
| [`tls_verify_explained`](#tls-verify-explained) | function | What a verify result means, for the codes a caller actually meets. The number is kept in the message because it is what  |
| [`tls_connect`](#tls-connect) | function | Open TCP, then TLS, verifying the chain AND the hostname. Refuses with a reason a reader can act on rather than answerin |
| [`tls_write`](#tls-write) | function | — |
| [`tls_read`](#tls-read) | function | Up to `limit` bytes. `None` at end of stream or on error, which a caller reads as "stop". |
| [`tls_close`](#tls-close) | function | — |
| [`https_send_request`](#https-send-request) | function | One request over TLS, and the response. The same shape as `http_send_request` and deliberately so: a caller moving from  |
| [`https_get`](#https-get) | function | — |
| [`https_post`](#https-post) | function | — |

## Types
{: #types}

### `TlsLink`
{: #tlslink}

```burxt
class TlsLink { context: CPointer, ssl: CPointer, fd: Int, protocol: String }
```

An open, VERIFIED connection. There is no way to make one that is not verified: `tls_connect` is the only constructor and it refuses rather than answering an unverified link.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/tls.bx#L70)

## Functions
{: #functions}

### `tls_verify_explained`
{: #tls-verify-explained}

```burxt
pure function tls_verify_explained(code: Int) -> String allocates
```

What a verify result means, for the codes a caller actually meets. The number is kept in the message because it is what an OpenSSL manual is indexed by.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/tls.bx#L74)

### `tls_connect`
{: #tls-connect}

```burxt
function tls_connect(a: Int, b: Int, c: Int, d: Int, port: Int, host: String)
```

Open TCP, then TLS, verifying the chain AND the hostname. Refuses with a reason a reader can act on rather than answering a link that might be worthless.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/tls.bx#L96)

### `tls_write`
{: #tls-write}

```burxt
function tls_write(link: TlsLink, text: String) -> Option<Int> touches network
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/tls.bx#L155)

### `tls_read`
{: #tls-read}

```burxt
function tls_read(link: TlsLink, limit: Int) -> Option<String> allocates touches network
```

Up to `limit` bytes. `None` at end of stream or on error, which a caller reads as "stop".

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/tls.bx#L162)

### `tls_close`
{: #tls-close}

```burxt
function tls_close(link: TlsLink) -> Bool touches network
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/tls.bx#L178)

### `https_send_request`
{: #https-send-request}

```burxt
function https_send_request(a: Int, b: Int, c: Int, d: Int, port: Int, host: String,
```

One request over TLS, and the response. The same shape as `http_send_request` and deliberately so: a caller moving from HTTP to HTTPS changes the function name and the port.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/tls.bx#L190)

### `https_get`
{: #https-get}

```burxt
function https_get(a: Int, b: Int, c: Int, d: Int, host: String, path: String)
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/tls.bx#L234)

### `https_post`
{: #https-post}

```burxt
function https_post(a: Int, b: Int, c: Int, d: Int, host: String, path: String,
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/tls.bx#L240)


{% endraw %}

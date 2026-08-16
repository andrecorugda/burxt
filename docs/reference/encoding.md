---
layout: doc
title: lib/encoding.bx
section: reference
description: "Hex, base64 and base64url, and every decoder REFUSES rather than guesses."
---

{% raw %}

# `lib/encoding.bx`

Hex, base64 and base64url, and every decoder REFUSES rather than guesses.

```burxt
use "lib/encoding.bx";
```

```burxt
 print(encoding_hex_encode(digest));                  // two lowercase digits per byte
 print(encoding_base64_encode(digest));               // padded, `+` and `/`
 print(encoding_base64url_encode(digest));            // unpadded, `-` and `_`
```

```burxt
 match encoding_base64_decode(from_the_wire) {
     None => { print("that was not base64"); }
     Some(bytes) => { received(bytes); }
 }
```

---- the one decision that shapes the whole file -------------------------------------------

**A decoder answers `Option`, and it says None for anything it was not given.** Not the bytes it could salvage, not an empty array, not the input with the bad characters dropped. A base64 decoder that skips what it does not recognise is the exact shape of the silent wrong answer this language exists to refuse: the caller gets a shorter key, a truncated signature or half a message, and every one of those looks like data rather than like an error.

Concretely, all four of these are refusals rather than best-effort answers:

```burxt
 encoding_base64_decode("Zm9v YmFy")   None — a space is not in the alphabet
 encoding_base64_decode("Zm9vYmF")     None — 7 characters is not a whole number of quanta
 encoding_base64_decode("Zg=v")        None — padding in the middle
 encoding_base64_decode("Zh==")        None — "f" is "Zg==", and "Zh==" is the same byte
                                              spelled with four bits of junk left over
```

The last one is the one people argue about, so it is argued here rather than in a commit message. RFC 4648 §3.5 lets a decoder either reject non-canonical trailing bits or ignore them, and **ignoring them means one byte string has 16 different spellings** — which breaks every use where the encoded form is the identity: a cache key, a deduplication set, a signed token, an `==` between two fingerprints. So they are checked, and `Zh==` is refused.

---- what is NOT accepted, said out loud so nobody discovers it -----------------------------

**Whitespace and line breaks are refused, including a trailing newline.** MIME (RFC 2045) wraps base64 at 76 columns and PEM at 64, so a certificate body or an email attachment does NOT decode here as it stands. That is deliberate — a decoder that skips whitespace also skips it in the middle of a quantum, which turns a corrupted transfer into a plausible-looking answer. The fix at the call site is one line and a reviewer can see it:

```burxt
 let flat: String = string_replace(string_replace(pem, "\n", ""), "\r", "");
```

**The two alphabets are kept apart.** `encoding_base64_decode` refuses `-` and `_`, and `encoding_base64url_decode` refuses `+` and `/`. A decoder that took either would accept a string that is neither dialect, and would silently paper over a caller that encoded with the wrong one.

---- padding: standard pads, url does not, and both decoders say why ------------------------

`encoding_base64_encode` always pads and its decoder REQUIRES padding: RFC 4648 §3.2 makes padding mandatory unless the data length is known by other means, and a length is exactly what is not known when bytes arrive from somewhere else.

`encoding_base64url_encode` does NOT pad, because `=` has to be percent-escaped in a query string and is illegal in a JWT, and every real user of base64url — JWT (RFC 7515 §2), WebAuthn, URL-safe identifiers — omits it. `encoding_base64url_decode` accepts **both**: unpadded because that is what this module emits, padded because RFC 4648 §5 permits it and other implementations send it. What it does not accept is a length that no encoding could have produced: after any padding is removed, a remainder of 1 character modulo 4 is refused, because one base64 character carries six bits and no whole number of bytes is six bits long.

---- bytes are `[Int]`, and the `_text` twin exists for the other half of the world ----------

The primary form of every function here works in `[Int]`, because that is what a hash digest, `file_read_bytes`, `c_bytes_at` and `secure_random_bytes` all answer, and encoding is a byte operation. The `_text` twin takes or answers a `String` for the case where the bytes came from `file_read` or are going into one.

A Burxt String really does hold arbitrary bytes — it carries its length in a header rather than ending at the first NUL — so `from_bytes([104, 0, 105])` is three bytes long and the middle one is a NUL. That is checked in `tests/pass/encoding_library.bx` rather than assumed, because the whole `_text` half of this module would be quietly wrong if it were not so.

---- §D0: a chunk list joined pairwise, and the threshold was measured ----------------------

Encoding produces one small String per output character and there is no way around that — the alphabet lookup is a `substring`. What there IS a way around is the flat `out = out + c` fold, which copies the whole answer once per character and made this project's compiler cost 1,132 MB where it now costs 169. So characters accumulate into a `chunk`, the chunk is pushed onto a `[String]` when it passes `ENCODING_CHUNK` bytes, and the list is joined by repeated PAIRWISE merge — `string_join_chunks`, which is `emit.bx`'s reference implementation under its public name.

**The size of that difference, measured on this module rather than quoted from B29.** Base64 of a 64 KB input, peak RSS by `/usr/bin/time -f "%M"`, the only change being whether the loop flushes into a chunk list or keeps appending to one flat String:

```burxt
 flat fold, out = out + piece      937,552 KB      0.45 s
 chunk list joined pairwise          6,864 KB      0.00 s
```

137× the memory for 64 KB, and it is quadratic, so 1 MB flat would not finish. That is the whole argument for §D0 in one table.

**The threshold was measured here rather than inherited**, because §D0 says one that worked for another caller is inert or wrong for the next. Base64 of 1 MB, peak RSS, best of three:

```burxt
 ENCODING_CHUNK      8      16      24      32      48      64      96     128
 peak RSS (MB)      99      95      93      94      95      97     101     106
```

The curve is flat from 16 to 48 and the minimum is at 24, by 0.6% over 32. **32 is chosen on that tie**, because it is `lib/string.bx`'s `STRING_CHUNK` and a second unexplained number in a second file is worth more than half a percent. Below 16 the pairwise merge has too many pieces to join; above 64 the chunk itself is being copied too many times as it grows.

The floor under all of this is about 90 MB for a 1 MB input, and it is not the fold: it is one small String allocation per output character, which the alphabet lookup needs and which nothing in this language can currently avoid. It is named here so nobody re-tunes the threshold looking for it.

`len(s)` on a String is one load from its header and not a scan, but the input length is still hoisted out of every loop condition below — the habit is what §D0 asks for, and the array `len` in a `while` is a real read either way.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`encoding_bytes_in_range`](#encoding-bytes-in-range) | function | Every element is a byte. A precondition rather than a mask: `bit_and(b, 255)` would encode 256 as a NUL and hand back so |
| [`encoding_hex_value`](#encoding-hex-value) | function | One hex digit's value, or -1. `-1` rather than an `Option` because this is the inner loop of the decoder and the caller  |
| [`encoding_hex_encode`](#encoding-hex-encode) | function | Bytes to lowercase hex. Two characters per byte, always — a leading zero is never dropped, because a hex string whose le |
| [`encoding_hex_encode_text`](#encoding-hex-encode-text) | function | The same, for bytes that arrived as a String. |
| [`encoding_hex_decode`](#encoding-hex-decode) | function | Hex back to bytes, or None. Refused: an odd number of characters, and any character that is not a hex digit — which incl |
| [`encoding_hex_decode_text`](#encoding-hex-decode-text) | function | Hex back to a String. `None` for exactly the inputs `encoding_hex_decode` refuses; it makes no UTF-8 promise about what  |
| [`encoding_base64_value`](#encoding-base64-value) | function | One base64 character's value, or -1. `sixty_two` and `sixty_three` are the alphabet's last two characters as bytes, so t |
| [`encoding_base64_encode_with`](#encoding-base64-encode-with) | function | The shared encoder. `alphabet` must be 64 characters; `pad` decides whether the final quantum is filled out to four char |
| [`encoding_base64_decode_with`](#encoding-base64-decode-with) | function | The shared decoder. Every refusal in this module's header is one of the `return Option.None` lines below, and they are i |
| [`encoding_base64_encode`](#encoding-base64-encode) | function | The precondition is repeated on the public wrappers rather than left to the kernel, because the signature a caller reads |
| [`encoding_base64_encode_text`](#encoding-base64-encode-text) | function | — |
| [`encoding_base64_decode`](#encoding-base64-decode) | function | — |
| [`encoding_base64_decode_text`](#encoding-base64-decode-text) | function | — |
| [`encoding_base64url_encode`](#encoding-base64url-encode) | function | — |
| [`encoding_base64url_encode_text`](#encoding-base64url-encode-text) | function | — |
| [`encoding_base64url_decode`](#encoding-base64url-decode) | function | — |
| [`encoding_base64url_decode_text`](#encoding-base64url-decode-text) | function | — |

## Functions
{: #functions}

### `encoding_bytes_in_range`
{: #encoding-bytes-in-range}

```burxt
pure function encoding_bytes_in_range(xs: [Int]) -> Bool
```

Every element is a byte. A precondition rather than a mask: `bit_and(b, 255)` would encode 256 as a NUL and hand back something that decodes cleanly and is not what the caller had.

It costs one extra walk of the array per call, which is what a precondition costs and what it is for. Callers whose bytes come from `to_bytes`, `c_bytes_at`, `file_read_bytes` or `secure_random_bytes` cannot fail it; a caller who built the array by hand can.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L147)

### `encoding_hex_value`
{: #encoding-hex-value}

```burxt
pure function encoding_hex_value(byte: Int) -> Int
```

One hex digit's value, or -1. `-1` rather than an `Option` because this is the inner loop of the decoder and the caller checks the sign on the next line — an `Option` per character would allocate nothing but would cost a `match` per nibble to say the same thing.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L170)

### `encoding_hex_encode`
{: #encoding-hex-encode}

```burxt
function encoding_hex_encode(bytes: [Int]) -> String
```

Bytes to lowercase hex. Two characters per byte, always — a leading zero is never dropped, because a hex string whose length is not twice its byte count cannot be decoded back.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L185)

### `encoding_hex_encode_text`
{: #encoding-hex-encode-text}

```burxt
function encoding_hex_encode_text(text: String) -> String
```

The same, for bytes that arrived as a String.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L209)

### `encoding_hex_decode`
{: #encoding-hex-decode}

```burxt
function encoding_hex_decode(text: String) -> Option<[Int]>
```

Hex back to bytes, or None. Refused: an odd number of characters, and any character that is not a hex digit — which includes a space, a trailing newline and the `0x` a caller might leave on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L215)

### `encoding_hex_decode_text`
{: #encoding-hex-decode-text}

```burxt
function encoding_hex_decode_text(text: String) -> Option<String>
```

Hex back to a String. `None` for exactly the inputs `encoding_hex_decode` refuses; it makes no UTF-8 promise about what it answers, the same way `from_bytes` does not.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L236)

### `encoding_base64_value`
{: #encoding-base64-value}

```burxt
pure function encoding_base64_value(byte: Int, sixty_two: Int, sixty_three: Int) -> Int
```

One base64 character's value, or -1. `sixty_two` and `sixty_three` are the alphabet's last two characters as bytes, so this function decides the dialect and refuses the other one: given `'+'` and `'/'` it answers -1 for `-` and `_`, and the other way round.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L259)

### `encoding_base64_encode_with`
{: #encoding-base64-encode-with}

```burxt
function encoding_base64_encode_with(bytes: [Int], alphabet: String, pad: Bool) -> String
```

The shared encoder. `alphabet` must be 64 characters; `pad` decides whether the final quantum is filled out to four characters with `=`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L280)

### `encoding_base64_decode_with`
{: #encoding-base64-decode-with}

```burxt
function encoding_base64_decode_with(text: String,
```

The shared decoder. Every refusal in this module's header is one of the `return Option.None` lines below, and they are in the order the header lists them.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L335)

### `encoding_base64_encode`
{: #encoding-base64-encode}

```burxt
function encoding_base64_encode(bytes: [Int]) -> String
```

The precondition is repeated on the public wrappers rather than left to the kernel, because the signature a caller reads is this one — it is what `burxt mcp-schema` publishes and what a contract failure names. The second walk of the array costs about 3 ms per megabyte against the encode's 90, which is what a precondition is worth.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L431)

### `encoding_base64_encode_text`
{: #encoding-base64-encode-text}

```burxt
function encoding_base64_encode_text(text: String) -> String
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L437)

### `encoding_base64_decode`
{: #encoding-base64-decode}

```burxt
function encoding_base64_decode(text: String) -> Option<[Int]>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L441)

### `encoding_base64_decode_text`
{: #encoding-base64-decode-text}

```burxt
function encoding_base64_decode_text(text: String) -> Option<String>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L445)

### `encoding_base64url_encode`
{: #encoding-base64url-encode}

```burxt
function encoding_base64url_encode(bytes: [Int]) -> String
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L458)

### `encoding_base64url_encode_text`
{: #encoding-base64url-encode-text}

```burxt
function encoding_base64url_encode_text(text: String) -> String
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L464)

### `encoding_base64url_decode`
{: #encoding-base64url-decode}

```burxt
function encoding_base64url_decode(text: String) -> Option<[Int]>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L468)

### `encoding_base64url_decode_text`
{: #encoding-base64url-decode-text}

```burxt
function encoding_base64url_decode_text(text: String) -> Option<String>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/encoding.bx#L472)


{% endraw %}

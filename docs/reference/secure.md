---
layout: doc
title: lib/secure.bx
section: reference
description: "Bytes nobody can predict, and a comparison that does not leak."
---


# `lib/secure.bx`

Bytes nobody can predict, and a comparison that does not leak.

```burxt
use "lib/secure.bx";
```

```burxt
 match secure_random_bytes(32) {
     None => { give_up("the kernel has no entropy to give"); }
     Some(key) => { use_it(key); }
 }
 if string_equals_constant_time(presented, expected) { ... }
```

Inside a function that itself answers an `Option`, `?` does the same in one line — `let key: [Int] = secure_random_bytes(32)?;` yields the bytes or returns `None` immediately.

---- this file is NOT lib/random.bx, and the two must never be confusable -------------------

`lib/random.bx` is a SEEDED generator. `random_from(seed)` replays the same sequence for the same seed, which is exactly right for a test, a shuffle, a sample or a simulation, and exactly wrong for a key, a token, a password or a nonce. This file is the opposite: every byte comes from the operating system's entropy pool, nothing is reproducible, and there is no seed to give.

The names carry that difference, because a reviewer reading a call site has nothing else to go on. Every function here says `secure`; nothing there does; and `random.bx`'s own header records the decision that a CSPRNG "gets its own type and its own file, so the two cannot be confused by eye." Nothing in this file should ever be reachable from a `Random`, and nothing there should ever be reachable from here.

---- E6: A SECRET CANNOT BE ZEROED IN BURXT. This is the file to read it in ------------------

There is no `zeroise`, no `explicit_bzero`, no destructor and no way to overwrite a String or an array in place and know the bytes are gone. A value lives until its region closes, and when the region closes its memory is released to the allocator **without being cleared**.

What follows from that, concretely:

* A key, a password or a session token stays readable in the process's heap for as long as

```burxt
 the region that holds it is open, and its bytes remain in freed memory afterwards.
```

* A core dump, a swapped page, a `/proc/<pid>/mem` read by the same user, or a later

```burxt
 allocation that happens to be handed the same pages can see it.
```

* Holding a secret in the SHORTEST-lived region you can is therefore the only mitigation this

```burxt
 language offers, and it is a real one — a secret read inside `region verify { ... }` is gone
 from the live heap when that block ends, even though the bytes are not scrubbed.
```

This is recorded as a 1.0 limitation in `spec/1.0/ROADMAP-1.0.md` §E6. It is written here as well, and at this length, because §E6 is a single table row and this file is where somebody handling a secret will actually be looking.

---- E5: what this file deliberately does NOT contain ----------------------------------------

No AES, no ChaCha20, no RSA, no Ed25519, no X25519, no TLS, no Argon2/scrypt/bcrypt. Those are §E5's BIND-do-not-hand-roll list, for two reasons that have not changed: Burxt has no control over instruction timing at the level a cipher needs, and RSA and the curves need arbitrary-precision integers, which do not exist here — `Decimal` is a scaled i64 capped at scale 18. A subtly wrong AES produces ciphertext that looks perfectly fine, which is the exact failure this language exists to prevent, so the answer is to call a reviewed C library through an `external function` rather than to write one in Burxt.

---- where the entropy comes from -------------------------------------------------------------

`getentropy(2)`, not `getrandom(2)`. Both fill a buffer from the kernel's pool; only one of them exists everywhere. `getrandom` is a Linux extension and the first macOS runner `tests/pass/os_random_bytes.bx` ever met refused to link it —

```burxt
 Undefined symbols for architecture arm64: "_getrandom"
```

— while `getentropy` is glibc >= 2.25, macOS >= 10.12, musl and the BSDs it came from. It answers 0 or -1 rather than a byte count, and it is **capped at 256 bytes per call**. That cap is in `secure_entropy_block`'s contract rather than discovered at run time, and the public `secure_random_bytes` loops over blocks so a caller asking for 4 KB of keystream never has to know the number 256 exists.

**The return value is checked, and that is the whole point of the check.** On failure the buffer keeps whatever `malloc` left in it — the previous allocation's contents, or zeroes — and returning that would hand back a "key" made of somebody else's freed memory. So a failed call answers `Option.None` and never a partial or unfilled array. Half a key is not a key.

`Option` rather than an empty array, deliberately: `os_random_bytes` in the fixture answers `[]` on failure, which is honest but is a value a caller can use by accident. `match` cannot be ignored.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`secure_entropy_block`](#secure-entropy-block) | function | One `getentropy` call's worth of bytes, or `None` if the kernel refused. |
| [`secure_random_bytes`](#secure-random-bytes) | function | `count` bytes from the operating system's entropy pool, or `None`. |
| [`secure_random_int_below`](#secure-random-int-below) | function | A uniform Int in `0 .. bound - 1`, or `None`. |
| [`secure_uuid_v4`](#secure-uuid-v4) | function | A version-4 UUID: `f81d4fae-7dec-4d0e-8fd0-1f6a1b2c3d4e`. |
| [`secure_token_hex`](#secure-token-hex) | function | `count` random bytes as lowercase hex — twice `count` characters, and safe in a URL, a header, a filename and a log line |
| [`secure_token_urlsafe`](#secure-token-urlsafe) | function | The same, in unpadded base64url — shorter for the same strength, and still safe in a URL. 32 bytes give 43 characters ag |
| [`string_equals_constant_time`](#string-equals-constant-time) | function | True when `a` and `b` hold the same bytes, in time that does not depend on where they differ. |
| [`bytes_equals_constant_time`](#bytes-equals-constant-time) | function | The same, for bytes. This is the one to use on a digest from `lib/hash.bx`, which answers `[Int]` — going through `from_ |

## Functions
{: #functions}

### `secure_entropy_block`
{: #secure-entropy-block}

```burxt
function secure_entropy_block(count: Int) -> Option<[Int]> touches input
```

One `getentropy` call's worth of bytes, or `None` if the kernel refused.

The 256-byte cap is in the CONTRACT, so a 257-byte request is a precondition failure with a position in it rather than a run-time -1 for a reason the signature never mentioned. Callers wanting more should use `secure_random_bytes`, which loops.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/secure.bx#L118)

### `secure_random_bytes`
{: #secure-random-bytes}

```burxt
function secure_random_bytes(count: Int) -> Option<[Int]> touches input
```

`count` bytes from the operating system's entropy pool, or `None`.

No upper bound: `getentropy`'s 256-byte cap is this function's problem and not the caller's, and the loop below is correct because separate calls are independently unpredictable — there is no stream state to keep and nothing to reseed.

If any block fails, the whole call fails. It does NOT answer the bytes it managed to collect: a short key is a weaker key that looks like a key.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/secure.bx#L146)

### `secure_random_int_below`
{: #secure-random-int-below}

```burxt
function secure_random_int_below(bound: Int) -> Option<Int> touches input
```

A uniform Int in `0 .. bound - 1`, or `None`.

**Not `remainder(draw, bound)`, and the difference is the reason this function exists.** Modulo on a 63-bit draw over a bound that does not divide 2^63 over-represents the low values — by a negligible amount for a die and by a factor of two for a bound just over 2^62, and by exactly the amount an attacker needs when the bound is a table size or a character-set length. So the draw is REJECTED and repeated when it lands in the short final stretch.

The draw is 63 bits rather than 64: eight bytes are read and the top bit is shifted away, which keeps every value non-negative without an `INT_MIN` special case. That costs one bit of the range and nothing else, since `bound` is an Int and cannot exceed `SECURE_INT_MAX` anyway.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/secure.bx#L178)

### `secure_uuid_v4`
{: #secure-uuid-v4}

```burxt
function secure_uuid_v4() -> Option<String> touches input
```

A version-4 UUID: `f81d4fae-7dec-4d0e-8fd0-1f6a1b2c3d4e`.

122 random bits. The other six are fixed by RFC 4122 §4.4 and are the two things a reader should check in the output: the first character of the third group is always `4` (the version), and the first character of the fourth group is always one of `8`, `9`, `a`, `b` (the variant, `10xx` in binary). `tests/pass/secure_library.bx` asserts both rather than eyeballing one sample.

Lowercase, and hyphenated in the 8-4-4-4-12 shape, because that is what RFC 4122 §3 says to emit even though it says to accept either case on input.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/secure.bx#L217)

### `secure_token_hex`
{: #secure-token-hex}

```burxt
function secure_token_hex(count: Int) -> Option<String> touches input
```

`count` random bytes as lowercase hex — twice `count` characters, and safe in a URL, a header, a filename and a log line.

The argument is a count of BYTES, not of characters, because the strength is in the bytes and a caller who reads "32" should get 256 bits. 32 is the number to use when unsure.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/secure.bx#L236)

### `secure_token_urlsafe`
{: #secure-token-urlsafe}

```burxt
function secure_token_urlsafe(count: Int) -> Option<String> touches input
```

The same, in unpadded base64url — shorter for the same strength, and still safe in a URL. 32 bytes give 43 characters against hex's 64.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/secure.bx#L245)

### `string_equals_constant_time`
{: #string-equals-constant-time}

```burxt
pure function string_equals_constant_time(a: String, b: String) -> Bool
```

True when `a` and `b` hold the same bytes, in time that does not depend on where they differ.

**Length is folded in rather than returned on.** The obvious `if len(a) != len(b) { return false; }` is an early exit, so instead the difference of the two lengths seeds the accumulator and the loop still runs. `b` is read at a WRAPPING index so a shorter `b` never goes out of bounds; when the lengths do match, the wrap never happens and this is an ordinary byte-for-byte comparison.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/secure.bx#L300)

### `bytes_equals_constant_time`
{: #bytes-equals-constant-time}

```burxt
pure function bytes_equals_constant_time(a: [Int], b: [Int]) -> Bool
```

The same, for bytes. This is the one to use on a digest from `lib/hash.bx`, which answers `[Int]` — going through `from_bytes` first would be a copy of the secret for no reason.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/secure.bx#L323)


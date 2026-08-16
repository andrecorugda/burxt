---
layout: doc
title: lib/hash.bx
section: reference
description: "Hashes and checksums."
---

{% raw %}

# `lib/hash.bx`

Hashes and checksums.

```burxt
use "lib/hash.bx";
```

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`hash_mask_32`](#hash-mask-32) | function | — |
| [`hash_rotate_right_32`](#hash-rotate-right-32) | function | Rotate a 32-bit word right by `n`, in a 64-bit register. |
| [`hash_rotate_right_64`](#hash-rotate-right-64) | function | Rotate a 64-bit word right by `n`. No mask, because 64 bits is the whole register — what leaves the top is exactly what  |
| [`hash_add_wrapping_64`](#hash-add-wrapping-64) | function | `a + b` discarding the carry out of bit 63 — the addition SHA-512 is defined on, which `+` cannot do because `+` traps. |
| [`hash_multiply_wrapping_64`](#hash-multiply-wrapping-64) | function | `a * b` keeping the low 64 bits — what FNV-1a's multiply-by-a-prime step needs. |
| [`hash_hex_digit`](#hash-hex-digit) | function | — |
| [`hash_hex`](#hash-hex) | function | A digest as lowercase hex — the form every standard prints its vectors in, and the form to compare against one. |
| [`hash_hex_int`](#hash-hex-int) | function | The low `width` bytes of `value` as lowercase hex, big-endian — for printing a `crc32` or an `fnv1a_64`, which answer an |
| [`hash_equals_constant_time`](#hash-equals-constant-time) | function | Whether two digests are equal, without an early exit. |
| [`crc32_byte`](#crc32-byte) | function | CRC-32/ISO-HDLC — the one in zip, gzip, PNG and Ethernet. Reflected, polynomial 0xEDB88320, initial and final xor 0xFFFF |
| [`crc32`](#crc32) | function | — |
| [`crc32_text`](#crc32-text) | function | — |
| [`fnv1a_32`](#fnv1a-32) | function | The multiply needs no wrapping helper: the hash is under 2^32 and the prime is under 2^25, so the product is under 2^57  |
| [`fnv1a_32_text`](#fnv1a-32-text) | function | — |
| [`fnv1a_64`](#fnv1a-64) | function | The 64-bit multiply overflows for almost every input, so it goes through the limb multiply. `hash_hex_int(fnv1a_64(...), |
| [`fnv1a_64_text`](#fnv1a-64-text) | function | — |
| [`sha256_k`](#sha256-k) | function | The sixty-four round constants: the first 32 bits of the fractional part of the cube root of each of the first 64 primes |
| [`sha256_block`](#sha256-block) | function | One 64-byte block, folded into the eight-word state `h`. |
| [`sha256`](#sha256) | function | SHA-256 of `data`, as 32 bytes. `hash_hex` turns it into the sixty-four characters a vector is printed as. |
| [`sha256_text`](#sha256-text) | function | — |
| [`sha512_k`](#sha512-k) | function | The eighty round constants: the first 64 bits of the fractional part of the cube root of each of the first 80 primes. ** |
| [`sha512_block`](#sha512-block) | function | One 128-byte block. Every addition goes through `hash_add_wrapping_64`, because at this width `+` traps — that single di |
| [`sha512`](#sha512) | function | SHA-512 of `data`, as 64 bytes. |
| [`sha512_text`](#sha512-text) | function | — |
| [`hmac_sha256`](#hmac-sha256) | function | — |
| [`hmac_sha256_text`](#hmac-sha256-text) | function | — |
| [`hmac_sha512`](#hmac-sha512) | function | — |
| [`hmac_sha512_text`](#hmac-sha512-text) | function | — |
| [`pbkdf2_sha256`](#pbkdf2-sha256) | function | — |
| [`pbkdf2_sha256_text`](#pbkdf2-sha256-text) | function | — |

## Functions
{: #functions}

### `hash_mask_32`
{: #hash-mask-32}

```burxt
pure function hash_mask_32(x: Int) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L164)

### `hash_rotate_right_32`
{: #hash-rotate-right-32}

```burxt
pure function hash_rotate_right_32(x: Int, n: Int) -> Int
```

Rotate a 32-bit word right by `n`, in a 64-bit register.

The mask is the whole point: `shift_left(x, 32 - n)` pushes bits into positions 32..63, which do not exist in the u32 this is pretending to be. They must be discarded here, before the caller xors this into anything.

`n == 0` would ask for `shift_left(x, 32)`, which is defined (it is a shift by less than the register width) and gives the right answer once masked. No round in SHA-256 rotates by 0.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L176)

### `hash_rotate_right_64`
{: #hash-rotate-right-64}

```burxt
pure function hash_rotate_right_64(x: Int, n: Int) -> Int
```

Rotate a 64-bit word right by `n`. No mask, because 64 bits is the whole register — what leaves the top is exactly what should be arriving at the bottom, and `bit_or` puts it there.

`n` must not be 0: `shift_left(x, 64)` shifts by the full register width, which the hardware does not define. No round in SHA-512 rotates by 0, and the precondition says so rather than leaving it to be discovered on some other machine.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L189)

### `hash_add_wrapping_64`
{: #hash-add-wrapping-64}

```burxt
pure function hash_add_wrapping_64(a: Int, b: Int) -> Int
```

`a + b` discarding the carry out of bit 63 — the addition SHA-512 is defined on, which `+` cannot do because `+` traps.

Split both into 32-bit halves and add the halves: each partial sum is at most 2^33, so no `+` here can trap. The carry out of the low half is bit 32 of `low`, which is why the high half adds `shift_right_zeros(low, 32)`. The carry out of the high half is discarded by the mask, which is what "wrapping" means.

Eleven operations, always the same eleven. `lib/math.bx`'s `math_wrapping_add` is a half-adder loop whose length depends on the operands — correct, general, and the wrong shape for a function called five hundred times per block.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L207)

### `hash_multiply_wrapping_64`
{: #hash-multiply-wrapping-64}

```burxt
pure function hash_multiply_wrapping_64(a: Int, b: Int) -> Int
```

`a * b` keeping the low 64 bits — what FNV-1a's multiply-by-a-prime step needs.

Sixteen-bit limbs, because that is the largest split whose partial products fit: two 16-bit values multiply to at most 2^32, and four of those sum to less than 2^34, so nothing here approaches i64's limit. Products whose place value is 2^64 or above vanish modulo 2^64 and are simply not computed — that is the `i + j <= 3` in the sum, written out as four groups.

`shift_left` discarding what leaves the top is doing real work in the last two terms: `s2 << 32` and `s3 << 48` both overflow the register, and dropping the excess is exactly correct modulo 2^64.

This is here rather than in `lib/math.bx` because `math_wrapping_mul` is shift-and-add over 64 bits, each step calling the looping `math_wrapping_add` — roughly 400 operations against these 30. Worth moving there one day; noted rather than done, because that file is not this one's.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L227)

### `hash_hex_digit`
{: #hash-hex-digit}

```burxt
pure function hash_hex_digit(value: Int) -> String
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L248)

### `hash_hex`
{: #hash-hex}

```burxt
pure function hash_hex(digest: [Int]) -> String
```

A digest as lowercase hex — the form every standard prints its vectors in, and the form to compare against one.

§D0: pieces into a chunk list, joined pairwise. A 32-byte digest never needs it; the loop that hashes a directory and hexes every answer does, and this is the code that gets copied.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L265)

### `hash_hex_int`
{: #hash-hex-int}

```burxt
pure function hash_hex_int(value: Int, width: Int) -> String
```

The low `width` bytes of `value` as lowercase hex, big-endian — for printing a `crc32` or an `fnv1a_64`, which answer an Int rather than a digest.

`fnv1a_64`'s answer is frequently negative, because a u64 above 2^63 is stored as the i64 with the same bits. Printing it as a decimal Int is legal and useless; printing it here gives the sixteen characters the FNV reference page prints.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L289)

### `hash_equals_constant_time`
{: #hash-equals-constant-time}

```burxt
pure function hash_equals_constant_time(a: [Int], b: [Int]) -> Bool
```

Whether two digests are equal, without an early exit.

**The name is the documentation and it is spelled out for the reason `shift_right_zeros` is.** The ordinary comparison stops at the first differing byte, so the time it takes tells an attacker how many leading bytes of a forged MAC were right — and a MAC can then be found one byte at a time instead of all at once. This reads every byte of both, always.

**It is not a timing guarantee and must not be sold as one.** Burxt does not control instruction selection, branch prediction or the cache, and nothing at this level can. What it removes is the data-dependent branch that is visible in the source, which is the mistake that actually gets made. Lengths are compared normally: the length of a digest is not a secret.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L311)

### `crc32_byte`
{: #crc32-byte}

```burxt
pure function crc32_byte(crc: Int, byte: Int) -> Int
```

CRC-32/ISO-HDLC — the one in zip, gzip, PNG and Ethernet. Reflected, polynomial 0xEDB88320, initial and final xor 0xFFFFFFFF.

**Promoted out of `tests/pass/bits.bx`, where it was written in v0.0.199 as proof that the new bit operations were enough to compute a real checksum.** It was correct there and is unchanged here, which is why this is `spec/1.0/ROADMAP-1.0.md` §E4 and the cheapest item on that board: the work was done, in the wrong place. The fixture keeps its own copy, deliberately — it is a demonstration that a reader can build a checksum from `bit_*` alone, and importing the answer from a library would delete the thing it demonstrates.

Bit-at-a-time, no table. A 256-entry table is four times faster and would have to be built per call, since a `const` cannot be an array; when a caller measures this as the bottleneck the table belongs in their loop, built once.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L339)

### `crc32`
{: #crc32}

```burxt
pure function crc32(data: [Int]) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L351)

### `crc32_text`
{: #crc32-text}

```burxt
pure function crc32_text(text: String) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L360)

### `fnv1a_32`
{: #fnv1a-32}

```burxt
pure function fnv1a_32(data: [Int]) -> Int
```

The multiply needs no wrapping helper: the hash is under 2^32 and the prime is under 2^25, so the product is under 2^57 and `*` cannot trap. Mask after, exactly as SHA-256 does.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L389)

### `fnv1a_32_text`
{: #fnv1a-32-text}

```burxt
pure function fnv1a_32_text(text: String) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L398)

### `fnv1a_64`
{: #fnv1a-64}

```burxt
pure function fnv1a_64(data: [Int]) -> Int
```

The 64-bit multiply overflows for almost every input, so it goes through the limb multiply. `hash_hex_int(fnv1a_64(...), 8)` is how to print one.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L409)

### `fnv1a_64_text`
{: #fnv1a-64-text}

```burxt
pure function fnv1a_64_text(text: String) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L418)

### `sha256_k`
{: #sha256-k}

```burxt
function sha256_k(mutable into: [Int]) -> Int
```

The sixty-four round constants: the first 32 bits of the fractional part of the cube root of each of the first 64 primes. Filled into an array the caller owns, because a function with no region-carrying parameter cannot return one and a `const` cannot be an array.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L437)

### `sha256_block`
{: #sha256-block}

```burxt
function sha256_block(mutable h: [Int], mutable w: [Int], message: [Int], at: Int, k: [Int]) -> Int
```

One 64-byte block, folded into the eight-word state `h`.

`w` is the caller's 64-entry scratch array and is overwritten here — it is passed in rather than allocated because this is called once per block and a megabyte is sixteen thousand blocks.

Every `+` in this function is between values under 2^32, in groups of at most five, so the largest intermediate is under 2^35 and none of them can trap. The `hash_mask_32` goes at the end of the sum, never between the terms.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L470)

### `sha256`
{: #sha256}

```burxt
function sha256(data: [Int]) -> [Int]
```

SHA-256 of `data`, as 32 bytes. `hash_hex` turns it into the sixty-four characters a vector is printed as.

The padding is the part worth reading twice, because it is where a hash that is right on "abc" goes wrong. The message gets one 0x80 byte, then zeros until the length is 56 modulo 64, then the length **in bits** as eight big-endian bytes. When the remainder is 56 or more, that fills one block and spills into a second — the `while` handles both without a special case, and the 64-byte and 119..128-byte fixtures are there to prove it.

Only the final block is built: whole blocks are read out of `data` where they lie.

(`tail` would have been the name; it is a keyword in this language.)

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L537)

### `sha256_text`
{: #sha256-text}

```burxt
function sha256_text(text: String) -> [Int]
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L584)

### `sha512_k`
{: #sha512-k}

```burxt
function sha512_k(mutable into: [Int]) -> Int
```

The eighty round constants: the first 64 bits of the fractional part of the cube root of each of the first 80 primes. **These were generated from the cube roots rather than transcribed** — 80 sixteen-digit constants is 1,280 characters and a single wrong one produces a hash that is wrong only for some inputs. The first (0x428a2f98d728ae22) and the last (0x6c44198c4a475817) are the two FIPS 180-4 prints in full, and they match.

Each value above 2^63 is a negative Int holding the right bits. Nothing here compares them.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L602)

### `sha512_block`
{: #sha512-block}

```burxt
function sha512_block(mutable h: [Int], mutable w: [Int], message: [Int], at: Int, k: [Int]) -> Int
```

One 128-byte block. Every addition goes through `hash_add_wrapping_64`, because at this width `+` traps — that single difference from `sha256_block` is what 64-bit words cost.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L633)

### `sha512`
{: #sha512}

```burxt
function sha512(data: [Int]) -> [Int]
```

SHA-512 of `data`, as 64 bytes.

The length field is **128 bits**, not 64 — eight zero bytes then the eight that carry the bit count. Writing only eight bytes here is the other classic SHA-512 bug: it produces a hash that is right for every input under 112 bytes and wrong for everything above, which the 896-bit fixture catches.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L697)

### `sha512_text`
{: #sha512-text}

```burxt
function sha512_text(text: String) -> [Int]
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L746)

### `hmac_sha256`
{: #hmac-sha256}

```burxt
function hmac_sha256(key: [Int], message: [Int]) -> [Int]
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L768)

### `hmac_sha256_text`
{: #hmac-sha256-text}

```burxt
function hmac_sha256_text(key: String, message: String) -> [Int]
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L803)

### `hmac_sha512`
{: #hmac-sha512}

```burxt
function hmac_sha512(key: [Int], message: [Int]) -> [Int]
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L807)

### `hmac_sha512_text`
{: #hmac-sha512-text}

```burxt
function hmac_sha512_text(key: String, message: String) -> [Int]
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L842)

### `pbkdf2_sha256`
{: #pbkdf2-sha256}

```burxt
function pbkdf2_sha256(password: [Int], salt: [Int], iterations: Int, length: Int) -> [Int]
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L877)

### `pbkdf2_sha256_text`
{: #pbkdf2-sha256-text}

```burxt
function pbkdf2_sha256_text(password: String, salt: String, iterations: Int, length: Int) -> [Int]
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/hash.bx#L924)


{% endraw %}

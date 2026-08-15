---
layout: doc
title: Builtins
section: reference
description: "Every call the language owns: what it answers, whether it allocates, and what it refuses."
---


# Builtins

The names a program may not declare, because the language already means something by them. The list comes from `is_reserved_name` in `src/rust-compiler/typeck.rs`; every signature below was **compiled** while this page was generated, so none of them is a signature the compiler would reject.

| Call | Answers | Allocates? | Reaches |
|---|---|---|---|
| [`print_error`](#print-error) | nothing | no | — |
| [`bit_and`](#bit-and) | `Int` | no | — |
| [`bit_or`](#bit-or) | `Int` | no | — |
| [`bit_xor`](#bit-xor) | `Int` | no | — |
| [`bit_not`](#bit-not) | `Int` | no | — |
| [`shift_left`](#shift-left) | `Int` | no | — |
| [`shift_right_zeros`](#shift-right-zeros) | `Int` | no | — |
| [`shift_right_sign`](#shift-right-sign) | `Int` | no | — |
| [`c_is_null`](#c-is-null) | `Bool` | no | — |
| [`c_string_at`](#c-string-at) | `String` | **yes** | — |
| [`c_bytes_at`](#c-bytes-at) | `[Int]` | **yes** | — |
| [`print`](#print) | nothing | no | — |
| [`len`](#len) | how many elements, or how many BYTES of a `String` | no | — |
| [`byte_at`](#byte-at) | the byte at `i` | no | — |
| [`byte_as_string`](#byte-as-string) | a one-byte `String` holding `n` | **yes** | — |
| [`substring`](#substring) | `count` bytes of `s`, starting at `from` | **yes** | — |
| [`to_string`](#to-string) | the value, written out | **yes** | — |
| [`push`](#push) | the new length | **yes** | — |
| [`truncate`](#truncate) | the new length | no | — |
| [`read_file`](#read-file) | the whole file | **yes** | `touches files` |
| [`write_file`](#write-file) | how many bytes went out | no | `touches files` |
| [`write_bytes`](#write-bytes) | how many bytes went out | no | `touches files` |
| [`argument`](#argument) | the nth command-line argument | no | `touches input` |
| [`argument_count`](#argument-count) | how many arguments there are | no | `touches input` |
| [`divide_floor`](#divide-floor) | `a / b`, rounded toward negative infinity | no | — |
| [`divide_toward_zero`](#divide-toward-zero) | `a / b`, rounded toward zero | no | — |
| [`remainder`](#remainder) | what is left over | no | — |
| [`hash`](#hash) | a non-negative hash | no | — |
| [`old`](#old) | what the expression was BEFORE the body ran | no | — |
| [`result`](#result) | the value being returned | no | — |
| [`exit`](#exit) | nothing — the program ends | no | — |
| [`main`](#main) | nothing — it is not an entry point | no | — |

*Allocates* means the call builds something, and a value has to be built somewhere. **It does not mean you write `region`.** Since v0.0.146 a program has one from the moment it starts and `allocates` is inferred, so every signature on this page compiles with no `region` in sight — the probes that verified them have none. You reach for `region` to release EARLY, around a loop body or a request, which is the whole of keeping a long-running program's memory flat. See [Memory](../guide/04-memory.html).

## `print_error`
{: #print-error}

```burxt
print_error(value)
```

Writes one value and a newline to **standard error**. The same statement as `print` with a different destination, so the per-type formatting cannot fork — two statements would mean two formatters, and the first time one learned about a new type the other would print something else.

**Answers** nothing. **Allocates:** no.

## `bit_and`
{: #bit-and}

```burxt
bit_and(a: Int, b: Int)
```

Bitwise AND. **Named rather than an operator**, because `a & b == c` means `a & (b == c)` in C — a precedence table a reviewer has to remember is the opposite of what this language is for.

**Answers** `Int`. **Allocates:** no.

## `bit_or`
{: #bit-or}

```burxt
bit_or(a: Int, b: Int)
```

Bitwise OR.

**Answers** `Int`. **Allocates:** no.

## `bit_xor`
{: #bit-xor}

```burxt
bit_xor(a: Int, b: Int)
```

Bitwise XOR.

**Answers** `Int`. **Allocates:** no.

## `bit_not`
{: #bit-not}

```burxt
bit_not(a: Int)
```

Every bit flipped. `bit_not(0)` is `-1`, because an `Int` is signed and there is nowhere else for the top bit to go.

**Answers** `Int`. **Allocates:** no.

## `shift_left`
{: #shift-left}

```burxt
shift_left(x: Int, n: Int)
```

Shifts left by `n`, which must be 0 to 63. Bits shifted past the top are **discarded** — the one place in this language where losing information is not an error, because it is what a shift is for. So it is **not** `x * 2^n`: multiplication traps on overflow and this does not.

**Answers** `Int`. **Allocates:** no.

## `shift_right_zeros`
{: #shift-right-zeros}

```burxt
shift_right_zeros(x: Int, n: Int)
```

Shifts right by `n`, filling with zeros — a logical shift. `shift_right_zeros(-1, 63)` is `1`. Two right shifts exist because on a negative value zero-fill and sign-fill give different answers, and one symbol cannot say which.

**Answers** `Int`. **Allocates:** no.

## `shift_right_sign`
{: #shift-right-sign}

```burxt
shift_right_sign(x: Int, n: Int)
```

Shifts right by `n`, copying the sign bit — an arithmetic shift. `shift_right_sign(-1, 63)` is `-1`, and `shift_right_sign(x, n)` equals `divide_floor(x, 2^n)`.

**Answers** `Int`. **Allocates:** no.

## `c_is_null`
{: #c-is-null}

```burxt
c_is_null(p: CPointer)
```

Did the C call fail? One of only **two** things that may be done with a `CPointer`. There is no `==` on one, no arithmetic and no printing — a pointer is a token to hand back to C, not a value to reason about.

**Answers** `Bool`. **Allocates:** no.

## `c_string_at`
{: #c-string-at}

```burxt
c_string_at(p: CPointer)
```

Copies NUL-terminated bytes from C into a Burxt `String`. **The copy is the wall**: afterwards Burxt owns the bytes and the pointer is not kept, so who frees it stops being a question. A null pointer dies here rather than answering `""` — unset and empty are different facts.

**Answers** `String`. **Allocates:** yes.

## `c_bytes_at`
{: #c-bytes-at}

```burxt
c_bytes_at(p: CPointer, n: Int)
```

Copies `n` bytes from C into a growable array, one byte per element, zero-extended so `0xFF` arrives as `255` rather than `-1`. The counterpart to `c_string_at`, and it is what makes OS entropy reachable: `/dev/urandom` is a character device, so `read_file` measures it and gets nothing. **Where the length comes from is the pointer wall's one soft edge** — `n` is your claim, and nothing in the type can check it. A null pointer and a negative count are refused.

**Answers** `[Int]`. **Allocates:** yes.

## `print`
{: #print}

```burxt
print(value)
```

Writes one value and a newline. Takes an `Int`, a `Bool`, a `String` or a `Decimal<S>`, and an interpolated string goes out piece by piece with nothing built — which is why printing costs no memory.

**Answers** nothing. **Allocates:** no.

## `len`
{: #len}

```burxt
len(xs) -> Int
```

On a `String` this counts bytes, not characters. That is deliberate and it is the same decision `byte_at` makes: the byte-versus-character question is one a program has to answer, and a name that hid it would answer it wrongly for somebody.

**Answers** how many elements, or how many BYTES of a `String`. **Allocates:** no.

## `byte_at`
{: #byte-at}

```burxt
byte_at(s, i) -> Int
```

Bounds are always checked. The name says BYTE so that nothing has to guess whether an index into text means a byte or a character.

**Answers** the byte at `i`. **Allocates:** no.

## `byte_as_string`
{: #byte-as-string}

```burxt
byte_as_string(n) -> String
```

**The exact inverse of `byte_at`**: `byte_at(byte_as_string(n), 0)` is `n` for every one of the 256 values. `n` must be 0 to 255 — a literal outside that is refused when the program is compiled, and anything computed is checked when it runs.

It is the ONLY way to turn a number into text, and the reason it had to be a builtin rather than a library function: `substring` of a literal was the only Int-to-String path there was, and a source file must be valid UTF-8, so a byte above 127 could only be written down inside a complete multi-byte character. `to_string(233)` is a different conversion — three digit characters, `"233"`.

**It is also the one builtin that can build a `String` `is_valid_utf8` rejects.** `byte_as_string(0xC3)` on its own is a UTF-8 lead byte with no continuation after it. That is what it is FOR — assembling a sequence one byte at a time — but it means the caller owns the validity of what comes out. For text, use `from_codepoint` in `lib/string.bx`, which emits a whole character or refuses.

A zero byte is ORDINARY, not a terminator: a Burxt `String` carries its length in a header, so `byte_as_string(0)` has length 1 and the full 0..255 range needs no special case.

**Answers** a one-byte `String` holding `n`. **Allocates:** yes.

It refuses this:

```burxt
print(byte_as_string(256));
```

```
error: `byte_as_string(256)` has no answer: a byte is 0 to 255. A codepoint above 255 is more than one byte — `from_codepoint` in lib/string.bx encodes it
 --> byte_as_string.bx:1:7
  |
1 | print(byte_as_string(256));
  |       ^^^^^^^^^^^^^^^^^^^
```

## `substring`
{: #substring}

```burxt
substring(s, from, count) -> String
```

Builds a new String, so it needs somewhere to put it.

**Answers** `count` bytes of `s`, starting at `from`. **Allocates:** yes.

## `to_string`
{: #to-string}

```burxt
to_string(value) -> String
```

Takes an `Int`, a `Bool` or a `Decimal<S>`. It shares its formatter with `print`, so the two can never disagree about what a number looks like. A `Bool` allocates nothing, because both answers are constants.

There is no way for a class of yours to have one: `to_string` is a builtin rather than an interface a type can implement, so a user type has no display form. That is a real gap rather than a decision.

**Answers** the value, written out. **Allocates:** yes.

It refuses this:

```burxt
let s: String = "already text";
print(to_string(s));
```

```
error: to_string(...) on a String would just copy it — use the value directly.
 --> to_string.bx:2:7
  |
2 | print(to_string(s));
  |       ^^^^^^^^^^^^
```

## `push`
{: #push}

```burxt
push(xs, value) -> Int
```

Appends to a growable array. The array lives in a region, which is what makes growing it a bump rather than a reallocation someone has to own.

**Answers** the new length. **Allocates:** yes.

## `truncate`
{: #truncate}

```burxt
truncate(xs, n) -> Int
```

Shortens a growable array to `n`. Nothing is freed — the region owns the storage, and it goes when the region does.

**Answers** the new length. **Allocates:** no.

## `read_file`
{: #read-file}

```burxt
read_file(path) -> String
```

Reaches the filesystem, so it is registered as `touches files` — a function that calls it must say so, and one that does not may not call it.

**Answers** the whole file. **Allocates:** yes. **Carries** `touches files`, so a caller must declare it.

## `write_file`
{: #write-file}

```burxt
write_file(path, contents) -> Int
```

Replaces the file. Reaches the filesystem, so it carries `touches files`.

**Answers** how many bytes went out. **Allocates:** no. **Carries** `touches files`, so a caller must declare it.

## `write_bytes`
{: #write-bytes}

```burxt
write_bytes(path, buffer) -> Int
```

Writes a growable `[Int]` where each element is one byte. For output that is not text — and a value outside 0–255 is a refusal rather than a truncation.

**Answers** how many bytes went out. **Allocates:** no. **Carries** `touches files`, so a caller must declare it.

## `argument`
{: #argument}

```burxt
argument(n) -> String
```

Reads the command line, which is `touches input`.

**Answers** the nth command-line argument. **Allocates:** no. **Carries** `touches input`, so a caller must declare it.

## `argument_count`
{: #argument-count}

```burxt
argument_count() -> Int
```

Reads the command line, which is `touches input`.

**Answers** how many arguments there are. **Allocates:** no. **Carries** `touches input`, so a caller must declare it.

## `divide_floor`
{: #divide-floor}

```burxt
divide_floor(a, b) -> Int
```

`Int / Int` is refused, because the two reasonable answers for a negative numerator disagree and an operator cannot ask which you meant. This is one of them; `divide_toward_zero` is the other. Dividing by zero stops the program.

**Answers** `a / b`, rounded toward negative infinity. **Allocates:** no.

## `divide_toward_zero`
{: #divide-toward-zero}

```burxt
divide_toward_zero(a, b) -> Int
```

What C and most CPUs do. The other half of the pair `Int / Int` refuses to guess between.

**Answers** `a / b`, rounded toward zero. **Allocates:** no.

## `remainder`
{: #remainder}

```burxt
remainder(a, b) -> Int
```

Keeps the sign of its LEFT operand, like C's `%`. Worth knowing when the result indexes something: a negative index is a bounds failure, not a wrong answer.

**Answers** what is left over. **Allocates:** no.

## `hash`
{: #hash}

```burxt
hash(key) -> Int
```

Defined on the `Equatable` types — `Int`, `Bool`, `String`, `Decimal<S>` — which is exactly the set `==` works on. The sign bit is cleared, so it can index a table through `remainder` without producing a negative index. This is the only compiler support `lib/map.bx` needs.

**Answers** a non-negative hash. **Allocates:** no.

## `old`
{: #old}

```burxt
old(expression)
```

Legal only inside an `ensures` clause, where it is what lets a postcondition talk about change rather than only about the answer.

One limit worth knowing, and it is the compiler's rather than the design's: an `ensures` on a method that returns a **class** is refused today. A class travels back through a hidden pointer into the caller's storage, and binding `result` to that needs care a scalar does not. Return a scalar, or drop the clause.

**Answers** what the expression was BEFORE the body ran. **Allocates:** no.

It refuses this:

```burxt
class Counter {
    n: Int,

    function (self) bumped() -> Counter
        ensures result.n == old(self.n) + 1
    { return Counter { n: self.n + 1 }; }
}
```

```
error: `ensures` on `Counter.bumped` is not supported yet: it returns a Counter, which travels through a hidden pointer into the caller's storage, so binding `result` to it needs care a scalar does not. Return a scalar, or drop the clause.
 --> old.bx:5:17
  |
5 |         ensures result.n == old(self.n) + 1
  |                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

## `result`
{: #result}

```burxt
result
```

A name, not a call, and in scope only inside an `ensures` clause. A declaration that takes a parameter called `result` and also writes `result` in an `ensures` is refused for the collision rather than shadowed quietly.

**Answers** the value being returned. **Allocates:** no.

## `exit`
{: #exit}

```burxt
exit(code)
```

Reserved because the runtime calls libc's `exit` to end a program on a failed contract or a bounds violation. A program that shadowed it would change what a failure does.

**Answers** nothing — the program ends. **Allocates:** no.

## `main`
{: #main}

```burxt
main
```

Burxt has no entry point: the whole file is the program, and statements at the top level run in order. So a function called `main` would look like an entry point and not be one — a trap rather than a crash, which is the kind of thing this language refuses. The name is reserved so it cannot be set.

**Answers** nothing — it is not an entry point. **Allocates:** no.


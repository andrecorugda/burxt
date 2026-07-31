---
layout: doc
title: Builtins
section: reference
description: Every call the language owns: what it answers, whether it allocates, and what it refuses.
---


# Builtins

The names a program may not declare, because the language already means something by them. The list comes from `is_reserved_name` in `src/typeck.rs`; every signature below was **compiled** while this page was generated, so none of them is a signature the compiler would reject.

| Call | Answers | Allocates? | Reaches |
|---|---|---|---|
| [`print`](#print) | nothing | no | — |
| [`len`](#len) | how many elements, or how many BYTES of a `String` | no | — |
| [`byte_at`](#byte-at) | the byte at `i` | no | — |
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


# Burxt — Absence and failure (M8)

> Status: **types DONE (v0.0.94), `?` DONE (v0.0.97).** `Option<T>` and `Result<T, E>` live in
> `lib/option.bx` and `lib/result.bx` as ordinary Burxt with no compiler support beyond M7's
> generics — which was the point: if `Option<T>` had needed a keyword, the generics were not
> finished. `?` is the one piece of syntax, and §1a records the two decisions it needed.
>
> Original status: **specified, to implement after M7.**

## 0. What has to become possible

```text
function find(haystack: String, needle: String) -> Option<Int> { ... }

match find(text, "burxt") {
    Some(at) => { print("found at {at}"); }
    None => { print("not there"); }
}
```

```text
function parse_amount(text: String) -> Result<Decimal<2>, String> { ... }

match parse_amount(input) {
    Ok(amount) => { print(amount); }
    Error(why) => { print("bad amount: {why}"); }
}
```

Today `lib/string.bx`'s `string_to_int("abc")` answers `0`, and `file_read` of a missing file answers
`""` — indistinguishable from success. Every such function is a lie the caller cannot detect.

## 1a. `?` — the two decisions it needed

`?` was named as blocked for three versions, on one question: **what happens when the callee's
error type is not the caller's?** Both answers are now written down.

### Decision A — no conversion. The error types must match.

```text
function parse_amount(text: String) -> Result<Decimal<2>, String> { ... }

function read_invoice(path: String) -> Result<Decimal<2>, String> {
    let amount = parse_amount(read_file(path))?;      // both errors are String
    return Result.Ok(amount);
}
```

If the two differ, the `match` is written out. **No `From`-like conversion trait**, no implicit
widening into a common error type, and no `Box<dyn Error>` — each of those is a mechanism that
decides on your behalf which information about a failure survives, and every one of them is a
place where a cause quietly becomes "something went wrong".

**Earns its place when:** a real program has two error enums it genuinely needs to bridge, and
the conversion is worth naming. Then it is a trait with one method, declared per pair — not a
blanket rule.

The cost, stated: a function that calls two libraries with different error types writes two
`match`es. That is more typing and it is also the honest amount of thinking, because somebody
has to decide what the caller's failure means.

### Decision B — `?` is spelled by VARIANT name, not by type name.

`?` works on any enum with **exactly two variants**, one of which is named `Error` or `None`. The
other variant carries the value, and its name is irrelevant. The enum's own name is irrelevant
too.

**Why not bless `Result` and `Option` by name.** They are library types. A compiler that knows
the *type* names cannot be told that `lib/` wrote them — it has hardcoded a specific library,
and a second library with the same shape is a second-class citizen. Blessing two *variant*
names is a much smaller commitment: it says "an enum whose failure case is called `Error` behaves
like a failure", which is a convention a reader already holds, and any library may follow it.

So `?` on a value of

```text
enum Fetched<T> { Error(String), Got(T) }
```

works, and reads correctly, without `Fetched` being known to the compiler.

**The enclosing function must return an enum with the same failure variant and the same payload
type.** That is Decision A, checked where `?` is written:

```text
error: `?` returns the error from the enclosing function, and `read_invoice` returns
       Result<Decimal<2>, Int> — this failure carries a String. `?` does not convert
       between error types: write the `match`, or make the two agree.
```

## 1. Decisions

### Decision 1 — they are enums in a library, not types in the compiler

```text
enum Option<T> { None, Some(T) }
enum Result<T, E> { Ok(T), Error(E) }
```

Four lines. Burxt already has enums with payloads and **exhaustive `match` with no
wildcard**, which is the whole mechanism: a `Result` you did not handle is a match missing
an arm, and that is already a compile error.

**Why not built in.** A compiler-blessed `Option` would need blessing for every operation on
it — mapping, defaulting, chaining — and each blessing is a rule a reader has to learn. As a
library type its behaviour is written in Burxt and can be read.

### Decision 2 — no `?`, no exceptions, no unwrap-by-default

Handling is a `match`. There is no operator that propagates a failure invisibly and no
function that turns an `Error` into a crash without saying so.

**Why no `?`.** It is genuinely convenient and it hides a control-flow edge at every call
site. Burxt's position on hidden control flow is already settled — no exceptions, `dynamic` only
where written, `tail` only where written — and `?` is the same question. Deferred with a
trigger rather than refused forever: **when a real program's error handling is more `match`
than logic, `?` earns a design.**

`option_or(default)` and `result_or(default)` exist as library functions, because *choosing*
a default is a decision the caller writes down. What does not exist is one that panics.

### Decision 3 — the library grows the obvious helpers, in Burxt

`option_is_some`, `option_or`, `result_is_ok`, `result_or`, `result_error_or` — each a
`match`, each three lines, none privileged. A caller who wants something else writes it.

### Decision 4 — the standard library is rewritten to use them

`string_to_int` answers `Option<Int>`. `file_read` answers `Result<String, String>`. That is the
acceptance test: the library's honest-limitation comments disappear because the limitation
does.

## 2. What this must NOT do

- **NO null.** There is none and there will be none. `Option<T>` is the absence of a value;
  a pointer that might not point is not a thing Burxt has.
- **NO implicit conversion** between `Option<T>` and `T`, in either direction.
- **NO `?` in this slice.** See Decision 2.
- **NO unwrap that panics.** A program that wants to stop on failure writes the `match` and
  the stop, so the stop is visible.
- **NO compiler support.** If any of this needs a rule in the typechecker, the generics are
  incomplete and that is the bug to fix.

## 3. Acceptance

1. `Option<T>` and `Result<T, E>` are declared in `lib/`, in Burxt, with no compiler change.
2. A `match` that omits `None` is a compile error — the existing exhaustiveness rule, doing
   the work.
3. `string_to_int` answers `Option<Int>`, and `string_to_int("abc")` is distinguishable from
   `string_to_int("0")`.
4. `file_read` answers `Result<String, String>`, and a missing file is distinguishable from an
   empty one.
5. `examples/invoice.bx` reads its input through the new signatures and still prints the same
   numbers.
6. Both compilers, differential test green.

# Burxt — Absence and failure (M8)

> Status: **specified, to implement after M7.** These are library types, not language
> features, and that is the point: if `Option<T>` needs compiler support, the generics are
> not finished.

## 0. What has to become possible

```text
fn find(haystack: String, needle: String) -> Option<Int> { ... }

match find(text, "burxt") {
    Some(at) => { print("found at {at}"); }
    None => { print("not there"); }
}
```

```text
fn parse_amount(text: String) -> Result<Decimal<2>, String> { ... }

match parse_amount(input) {
    Ok(amount) => { print(amount); }
    Err(why) => { print("bad amount: {why}"); }
}
```

Today `lib/str.bx`'s `str_to_int("abc")` answers `0`, and `fs_read` of a missing file answers
`""` — indistinguishable from success. Every such function is a lie the caller cannot detect.

## 1. Decisions

### Decision 1 — they are enums in a library, not types in the compiler

```text
enum Option<T> { None, Some(T) }
enum Result<T, E> { Ok(T), Err(E) }
```

Four lines. Burxt already has enums with payloads and **exhaustive `match` with no
wildcard**, which is the whole mechanism: a `Result` you did not handle is a match missing
an arm, and that is already a compile error.

**Why not built in.** A compiler-blessed `Option` would need blessing for every operation on
it — mapping, defaulting, chaining — and each blessing is a rule a reader has to learn. As a
library type its behaviour is written in Burxt and can be read.

### Decision 2 — no `?`, no exceptions, no unwrap-by-default

Handling is a `match`. There is no operator that propagates a failure invisibly and no
function that turns an `Err` into a crash without saying so.

**Why no `?`.** It is genuinely convenient and it hides a control-flow edge at every call
site. Burxt's position on hidden control flow is already settled — no exceptions, `dyn` only
where written, `tail` only where written — and `?` is the same question. Deferred with a
trigger rather than refused forever: **when a real program's error handling is more `match`
than logic, `?` earns a design.**

`option_or(default)` and `result_or(default)` exist as library functions, because *choosing*
a default is a decision the caller writes down. What does not exist is one that panics.

### Decision 3 — the library grows the obvious helpers, in Burxt

`option_is_some`, `option_or`, `result_is_ok`, `result_or`, `result_error_or` — each a
`match`, each three lines, none privileged. A caller who wants something else writes it.

### Decision 4 — the standard library is rewritten to use them

`str_to_int` answers `Option<Int>`. `fs_read` answers `Result<String, String>`. That is the
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
3. `str_to_int` answers `Option<Int>`, and `str_to_int("abc")` is distinguishable from
   `str_to_int("0")`.
4. `fs_read` answers `Result<String, String>`, and a missing file is distinguishable from an
   empty one.
5. `examples/invoice.bx` reads its input through the new signatures and still prints the same
   numbers.
6. Both compilers, differential test green.

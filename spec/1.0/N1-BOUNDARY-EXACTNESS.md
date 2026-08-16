# Burxt — Exactness That Survives the Boundary (NOVELTY §1, slice 1)

> Status: **SHIPPED in v0.0.30** (see §6 for the verified acceptance). This is
>
> **Corrected 2026-08-16 — see §7.** This document said the one boundary that exists today is
> the C FFI. That was incomplete from v0.0.28: `to_string` renders a `Decimal` through the host's
> `snprintf`, so the most-used function in the language is a boundary nobody had counted. **No
> platform Burxt ships to renders it differently** — `%0Nllu` has one answer in any conforming
> libc — so this is a narrowing of where the boundary is, not a defect report. A status of
> SHIPPED is exactly what stops a reader reaching a correction, which is why it is up here.
> the first slice of `NOVELTY.md` §1 — labelled there as *"the strongest
> unclaimed territory, and the one I would bet on first"*, novelty high,
> buildability high.

## 0. The claim this is chasing

> **Exact end-to-end, not merely exact in memory.**

A `Decimal<2>` is exact inside Burxt. Every language guards the arithmetic and
then abandons the wire: the value is serialized to a JSON number, or bound to a
`float` column, or handed to a C function taking a `double`, and the guarantee
evaporates in silence. **Real financial defects overwhelmingly live at
boundaries, not in arithmetic.**

Burxt already refuses lossy narrowing *inside* the language. This extends the
same refusal outward.

## 1. Where the boundary actually is today

Honesty first: Burxt has no serializer and no database driver, so there is no
encoder to guard yet. The one boundary that exists today is the **C FFI**, and
its current state is a blanket refusal:

```text
external function log_amount(amount: Decimal<2>) -> CInt;
// error: only Int, CInt and String may cross the C boundary for now
```

That refusal is safe but **it is not the claim.** "Decimals cannot cross" is a
missing feature; "Decimals cross only through an encoding that cannot lose
them" is a guarantee. This slice converts the first into the second.

## 2. Decisions

### Decision 1 — `CDouble`, an FFI-only type that models C's `double` honestly

The same move `CInt` made for C's `int`. Without a name for the lossy foreign
type, "a Decimal may not bind to a float" is **unspellable**, so the guarantee
cannot be checked — it is merely absent. With it, the rule becomes a type rule.

`CDouble` exists ONLY in `external function` signatures, exactly like `CInt`. Burxt
still has no float type of its own and this does not introduce one.

### Decision 2 — `Decimal<S>` → `CDouble` is a compile error, always

No flag, no opt-in, no "I know what I'm doing" escape. The message names the
concrete loss rather than citing a rule:

```text
error: a C `double` cannot hold a Decimal<2> exactly — $0.10 is not
       representable in binary floating point, so this crossing would silently
       change the amount. Declare the parameter as `Decimal<2> as scaled` to
       pass the exact scaled integer, or pass `to_string(amount)` as text.
```

This is the headline of the slice: **the guarantee extends past the language
edge**, and the error teaches the two exact alternatives.

### Decision 3 — `Int` → `CDouble` is allowed, but range-checked at runtime

A double holds every integer up to 2^53 exactly and starts skipping them after
that. So an `Int` may cross as a `CDouble`, checked: `|n| > 2^53` is a named
runtime error with exit 70, like every other narrowing failure in the language.
Silently handing C a different integer than the one written would be the same
class of defect as a silent decimal rounding.

### Decision 4 — a Decimal crosses through a marshaller declared at the DECLARATION site

```text
external function log_amount(amount: Decimal<2> as scaled) -> CInt;

region r { print(log_amount($19.99)); }   // C receives 1999
```

`as scaled` says: the C side receives **the exact unscaled integer**. Nothing is
converted, nothing rounds, and the reading is plain English — *"a Decimal<2>,
crossing as a scaled integer."*

**Why the declaration site and not the call site.** The obvious alternative is a
call-site accessor: `log_amount(scaled_of(price))` with `log_amount` taking an `Int`.
That is strictly weaker, and weaker in exactly the way §1 is about: the scale is
gone from the type, so a `Decimal<4>`'s unscaled integer type-checks
identically, and so does an unrelated `Int`. **The scale is lost at the boundary
— which is the defect, not the fix.** Declared at the declaration site, the
scale is part of the contract: it is checked once, `Decimal<4>` is refused, and
every call site is then correct by construction.

A marshaller is meaningless on a non-Decimal, and `as scaled` on an ordinary
`function` parameter is noise (a Burxt-to-Burxt call has no encoding question). Both
are compile errors.

### Decision 5 — no `as text` marshaller

It already exists as a composition: `c_fn(to_string(amount))` works today
(v0.0.28), passing a NUL-terminated exact decimal string. A feature whose only
contribution is a second spelling of something already expressible earns no
place. The `CDouble` error message points at `to_string` for exactly this
reason.

### Decision 6 — a `CDouble` RETURN stays refused

Burxt has no exact way to receive a real number, and inventing an inexact
receiver to complete the matrix would contradict the thesis. The error says how
to get the value exactly instead — have the C side return a scaled integer, or a
string:

```text
error: external function `rate` returns CDouble, but Burxt has no float type to receive
       it exactly. Have the C function return the scaled integer (and declare
       `-> Int`), or return it as text.
```

## 3. What this milestone must NOT do

- **NO implicit Decimal ↔ double conversion, ever, under any flag.** If this
  ever seems necessary, the answer is a wider scaled integer, not a float.
- **NO float type in Burxt.** `CDouble` appears only in `external function` signatures.
  A Burxt binding, field, parameter, or return may never have it.
- **NO `as` marshallers on ordinary `function` parameters.** Marshalling exists only
  where there is a foreign encoding to marshal into.
- **NO "close enough" mode** on the `Int` → `CDouble` range check. The check is
  not tunable.
- **NO serialization layer in this slice.** There is no encoder to guard yet;
  the rule lands at the FFI, where the boundary is real today. When an encoder
  is built it inherits these rules rather than inventing its own.
- **NO CFloat (32-bit).** `double` covers the C APIs that exist to call.

## 4. Deferred ledger

| Feature | Why deferred | Earns its place when |
|---|---|---|
| `CDouble` returns | No exact receiver exists | Burxt has an exact way to receive a real number |
| `as text` marshaller | Already expressible via `to_string` | A boundary needs text that `to_string` cannot serve |
| JSON / database encoders | No encoder exists to guard | An encoder is built — it inherits §2's rules |
| `CFloat` (32-bit) | `double` covers real C APIs | A required C API takes a `float` |
| Marshalled Decimal **returns** from C | Same ownership question as any C return | C returns are widened generally |

## 5. Acceptance

A program that:

1. declares `external function` taking `Decimal<2> as scaled` and receives the exact
   unscaled integer on the C side;
2. is refused when the same value is declared as `CDouble`, with a message
   naming the loss and both exact alternatives;
3. is refused when a `Decimal<4>` is passed where `Decimal<2> as scaled` is
   declared — the scale is part of the contract;
4. passes an `Int` as `CDouble` and gets the same number back out through C;
5. dies with a named error (exit 70) when that `Int` exceeds 2^53;
6. is refused when a marshaller is written on an ordinary `function` parameter, or on
   a non-Decimal external parameter, or when a `CDouble` is returned.

## 6. Acceptance, verified (v0.0.30)

All six criteria in §5 hold, checked by running the compiler:

1. `tests/pass/boundary_scaled_marshal.bx` passes `$19.99` to C's `llabs` through
   `Decimal<2> as scaled` and gets `1999`; a `Decimal<4>` goes to `labs` as `725`.
2. `tests/fail/boundary_decimal_into_double` — refused, naming the loss and both
   alternatives.
3. `tests/fail/boundary_scale_is_the_contract` — a `Decimal<4>` where
   `Decimal<2> as scaled` was declared is a type error.
4. `money_and_integers_cross_into_c_exactly` (tests/runner.rs) links hand-written
   C and checks the exact bytes both ways: `1999` and 2^53 unchanged.
5. The same test asserts 2^53 + 1 exits 70 with a named error.
6. `tests/fail/boundary_marshal_on_burxt_fn`, `..._on_non_decimal`,
   `..._on_struct_field`, `boundary_unknown_marshaller`,
   `boundary_cdouble_return` — each refused with its own explanation.

**One thing this slice needed that the spec did not anticipate:** linker
pass-through. An `external function` declaration is only half an FFI — the other half is
a real object to link against, and there was no way to supply one. Arguments
after the source file now go to the system linker unchanged
(`burxt run pay.bx cside.o -lm`). Without it the guarantee could only be
described, not tested against real C.

---

## 7. The boundary nobody was looking at — `to_string` (2026-08-16)

§1 said the one boundary that exists today is the C FFI, and named the encoder and the database
as the ones still to come. It missed one that had been open since v0.0.28, in the most-used
function in the language.

**A `Decimal` is rendered to text by the host's `snprintf`.**

```rust
// src/rust-compiler/codegen.rs:2360
let fmt_str = format!("%s%llu.%0{}llu", scale);   // sign, whole, zero-padded fraction
```

`:2341` is the same for scale 0, and `to_string(Int)` is the same shape. So the arithmetic is
exact — scaled integers, no float, from literal through every operation — and then **the last
step, the one that produces the characters a human reads, leaves the language.**

**And the narrower claim is the true one, so it is the one this section makes.**

`%llu` with a zero-pad width has exactly one answer in any *conforming* C library. It is
unsigned-integer-to-decimal-digits: no rounding, no float, no precision question, and integer
conversion is not locale-sensitive without the `'` flag, which Burxt never emits. On glibc, musl,
Apple's libc and MSVC the output is identical **by construction rather than by luck**.

So there is no latent defect on any platform Burxt ships to today, and it would be wrong to write
that the exactness thesis has always had a hole. The first draft of this section said so and it
was corrected before landing, by the person whose own finding it would have made more impressive.

**The exposure is precisely hosts that implement `printf` themselves** — and that is a surface
which did not exist before this month and is exactly where the language is going: a wasm island,
an embedded libc, a freestanding target with no libc at all. Asking every future host author to
get zero-padding right is asking for the defect below, and zero-padding is the one detail that
corrupts money silently instead of crashing.

### 7.1 It is not theoretical — measured, 2026-08-16

Found while compiling a BMX view to `wasm32-unknown-unknown` and calling it from JavaScript.
The host supplied its own `snprintf`, because a wasm island has no libc to borrow one from. Its
varargs walker read the conversion character and discarded the flags and width, so `%02llu`
behaved as `%llu`:

```text
native:  <p>Total: 1299.05</p>
wasm:    <p>Total: 1299.5</p>
```

**`$1299.05` rendered as `1299.5`.** A factor of ten, no crash, no warning, and nothing in Burxt
able to see it happen — by then the value had left the language.

**This was a non-conforming `printf`, not two platforms disagreeing**, and the section is careful
about that because the stronger reading is the one that would get quoted back for a year. What it
demonstrates is not that libcs differ; it is what the delegation costs the moment a host has to
supply the function itself.

The host was fixed within the hour. That is not the point either. The point is the shape: **it
survived three earlier probes** — a hello world, a `String`-only island and an escaping test —
because none of them formatted a fraction with a leading zero. A rule right about the case
someone wrote and silent about the case nobody did.

`DESIGN.md` calls a silent wrong answer worse than a crash. This one was produced by the
language's own flagship type, at the only boundary a reader ever actually sees.

### 7.2 The fix: render `Decimal` and `Int` in Burxt

A scaled integer to text is a digit loop and a zero-pad. The digit half already exists —
`lib/hash.bx:253` writes hex digits, and `lib/decimal.bx` already performs exact-integer descent
for `decimal2_cents`. Nothing here needs a compiler feature; it needs the compiler to stop
delegating.

**What it buys, in order of weight:**

1. **It removes the most error-prone thing a host shim is asked to do.** Zero-padding is the one
   detail that corrupts money rather than crashing it, and every future host author is currently
   asked to get it right. One of them already did not, on the first day anyone tried.
2. **The guarantee stops being conditional on the host.** Not because any current host breaks it —
   none does — but because "exact, provided your libc conforms" is a weaker sentence than "exact",
   and the difference costs nothing to close. It also becomes testable: a formatter written in
   Burxt is covered by the fixture suite on every target and by the stage-0/stage-1 agreement,
   where `snprintf` is covered by whoever wrote the host's libc.
3. **It removes the varargs walker from a wasm island entirely.** `snprintf` is the only reason
   an island needs one. Without it, a BMX island's whole host shim is `malloc`, `memcpy`, and
   three symbols that end the program — measured.
4. **One fewer libc symbol on every target**, and `%llu` against an `i64` is a portability
   assumption nobody has audited.

### 7.3 The cost, stated rather than discovered

It is compiler work in both stages, it must be byte-identical between them, and **getting a
zero-pad or a negative-zero case wrong reintroduces exactly this defect with our name on it
instead of a shim author's.** A formatter the two compilers disagree about is worse than
`snprintf`, which at least disagrees consistently across a single program.

That argues for the fixtures being written **before** the implementation, not after it.

### 7.4 The bar, which is also the version classification

A formatter written in Burxt **adds no surface** — no new function, no keyword, no flag. Under
`docs/compatibility.md` that makes it a **patch**, and that classification is the acceptance
test rather than a technicality:

> **If the output is not byte-identical to what glibc produces today, for every fixture below,
> it is not a patch and it is not done.**

The set, chosen for the cases that bite rather than the cases that are easy:

| Fixture | Why it is here |
|---|---|
| `$0.05` | the leading zero in the fraction — the exact shape that produced `1299.5` |
| `$1299.05` | the same, with a whole part, as measured |
| `-$0.05` | sign and leading zero together |
| `-$0.00` | negative zero: does the sign survive a zero magnitude, and *should* it |
| scale 0 | the `:2341` path, which has no fraction at all |
| scale 7 | the widest scale in use (`N9-VECTORS-EXACTLY.md`); scale 8 overflows |
| `INT_MAX`, `INT_MIN` | the unsigned-cast boundary — `%llu` on a negative `i64` today |
| largest value at each scale | where whole and fraction meet the width limit |

And per the same bar: **stage-0 and stage-1 must agree over the whole set**, not merely each
match glibc. Two implementations that both match glibc but not each other is a fixpoint failure
wearing a passing test.

### 7.5 Where the work is tracked

**This section is a correction, not a plan.** It lives here because §1 is wrong where a reader
finds it, and a spec wrong about its own boundary is worse than one that is merely out of date —
nobody re-checks a boundary a document has already located. The *work* is a row in
[`../ROADMAP-1.2.md`](../ROADMAP-1.2.md), which is where active work lives; `spec/1.0/` holds
what 1.0 shipped and must not quietly become a plan for what comes after it.

### 7.6 What this does not cover

`to_string(Bool)` and string concatenation do not reach `snprintf` and are unaffected. The
serializer and database boundaries §4 defers remain deferred — this section narrows §1's
statement of where the boundary is, and does not widen the milestone.

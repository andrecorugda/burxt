# Burxt — Exactness That Survives the Boundary (NOVELTY §1, slice 1)

> Status: **SHIPPED in v0.0.30** (see §6 for the verified acceptance). This is
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

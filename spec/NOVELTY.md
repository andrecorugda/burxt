# Burxt — Novelty Register

> Status: **ambition, deliberately not scoped.** This file exists to hold what
> Burxt is *for* — the things existing languages do not solve — separately from
> the milestone specs, which govern what gets built next.
>
> **Read the distinction, it matters.** "Resist the kitchen sink / every feature
> earns its place" is a rule about **implementation**. It is not a rule about
> **ambition**. Nothing in this file is scheduled; everything in it is meant to
> be reached. Applying shipping-discipline to vision produces incremental
> answers, which is the opposite of why this language exists.

Each candidate is labelled honestly on two axes — **how novel** (does anything
else do this?) and **how buildable** (research, or reachable from here?) —
because a novel thing that cannot ship is not yet novelty, it is a wish.

---

## The through-line: determinism

The candidates below look unrelated and are not. They are all the same claim in
different places:

> **The same inputs produce the same money, everywhere, provably — and nothing
> silently intervenes.**

Exact scaled integers give it *in memory*. Byte-identical semantics across
targets give it *across platforms*. What follows extends it across
**boundaries**, across **effects**, across **concurrency**, and across **cost**
(a guaranteed tail call is determinism about stack usage). That is a coherent
identity, not a list of features.

---

## 1. Exactness that survives the boundary

**Novelty: high. Buildability: high — reachable from what exists today.**
*The strongest unclaimed territory, and the one I would bet on first.*

A `Decimal<2>` is exact inside Burxt. Then it is serialized to JSON, becomes an
IEEE-754 double, and the guarantee silently evaporates. Or it is written to a
`float` database column. Or handed to a C function expecting a `double`.

**Real financial defects overwhelmingly live at boundaries, not in arithmetic.**
Every language guards the arithmetic and abandons the wire. Burxt already
refuses lossy narrowing *inside* the language; the novel move is extending that
same refusal outward:

- A `Decimal<2>` cannot cross a boundary through a lossy encoder. Serializing it
  as a JSON number is a compile error; as a string, or as a scaled integer plus
  its scale, is fine.
- Binding it to a `float`/`double` column or field is a compile error.
- Crossing into C or JS requires a *declared*, exactness-preserving marshaller —
  the same discipline the FFI already applies by refusing Decimal parameters.

**Why this is credible now:** it needs no new theory. It is type rules at the
FFI and serialization edge, built on machinery that exists (the FFI boundary
already refuses Decimals; `CInt` already proves the compiler can model a
foreign type's width honestly).

**The claim it earns:** *exact end-to-end, not merely exact in memory.* No
language can say this.

**Slice 1 SHIPPED in v0.0.30** — see `spec/N1-BOUNDARY-EXACTNESS.md`. What landed
is the FFI half, which is the only boundary that exists today: `CDouble` names
C's `double` so a lossy crossing can be refused rather than merely absent;
`Decimal<S>` → `CDouble` is a compile error naming the loss; `Int` → `CDouble` is
range-checked at 2^53; and a Decimal crosses only through a marshaller declared
on the SIGNATURE (`amount: Decimal<2> as scaled`), so the scale is part of the
contract instead of being lost in an `Int` at the call site. A test checks all of
it against hand-written C.

**Still open in §1:** serialization and database boundaries, which need an
encoder to exist before there is anything to guard. When one is built it inherits
these rules rather than inventing its own.

---

## 2. Provably deterministic money math (via forbidden effects)

**Novelty: high. Buildability: medium — needs an effect system first.**

If a function's effects are part of its type (see §6), they can be *forbidden*,
not merely documented:

```text
// illustrative, not current syntax
pure fn interest(balance: Decimal<2, RoundHalfEven>, rate: Decimal<4>) -> Decimal<2, RoundHalfEven>
```

A function so marked may not touch I/O, the clock, randomness, locale, or
ambient configuration — enforced by the compiler. Every input is a parameter;
every run is reproducible.

**Why this matters commercially, not just aesthetically:** financial auditors
and regulators care intensely whether a calculation is reproducible, and today
the honest answer in every language is "we believe so." A hidden
`DateTime.Now`, a locale-dependent parse, or a config lookup buried three calls
down silently makes a computation irreproducible, and nothing catches it.
Burxt could make it a compile error.

Composes directly with the existing byte-identical-across-targets property:
same inputs, same result, on web and desktop and mobile, *and* nothing hidden
feeding in.

**Slice 1 SHIPPED in v0.0.39** — `spec/N2-PURE-FUNCTIONS.md`. The illustrative
syntax above is the actual syntax. It needed **less** than an effect system: the
first declared effect marker arrived in v0.0.38 (`allocates`), and `pure` is the
same shape pointed the other way — one that forbids rather than permits. A `pure fn`
may not print, read a file, call into C, or call a function that is not `pure`.

Stated honestly: Burxt has no clock, no random, no locale and no ambient
configuration, so today the rule bites on I/O and the FFI, and is otherwise a
forward guarantee — a clock will be added *behind* it. Purity-driven optimisation is
deliberately excluded from the slice that introduces the marker, so it changes what
compiles and nothing else.

---

## 3. Contracts as conservation laws, with atomicity derived from them

**Novelty: very high. Buildability: low statically, medium at runtime.**
*The most novel and the least reachable — sequence it last.*

The contract grammar can express what money systems actually need, which is not
"this integer has no data race" but *"value is conserved"*:

```text
fn transfer(from: mut Account, to: mut Account, amount: Decimal<2>)
    requires amount > $0.00
    ensures  from.balance + to.balance == old(from.balance + to.balance)
```

That `ensures` is a **conservation law** — the invariant that actually defines
correctness for a ledger.

**The genuinely novel step is the consequence for concurrency.** Once the
compiler knows which state an invariant spans, it can **require mutual
exclusion over exactly that state** — *deriving* the atomicity from the declared
invariant, instead of asking the programmer to place locks correctly and hoping.
Software Transactional Memory gives atomic regions, but nothing ties atomicity
to a *declared invariant*, which is what makes it checkable rather than merely
available.

**Honest cost:** static proof of arbitrary contracts is SMT-solver territory
(Dafny, Why3, F\*). Runtime-checked contracts plus derived locking is reachable
much sooner and is worth shipping first; the A4.7 amendment already stages
contracts that way.

**The runtime staging SHIPPED in v0.0.43** — `spec/A5-CONTRACTS.md`. `requires` and
`ensures` are checked on entry and before every return, the failing clause is quoted
verbatim with the function's name, and a clause must be `pure` (a check that can
change the program is a second program). Contracts are always checked: there is no
build mode that strips them.

**What is still missing for §3 specifically, stated plainly:** `old(...)`, and
therefore conservation laws. Capturing pre-state only means something for functions
that mutate, which today means methods with a `mut self` receiver, and neither piece
exists yet. The novel step — *deriving* mutual exclusion from a declared invariant —
additionally waits on threads.

---

## 4. Guaranteed tail calls — a checked guarantee, not an invisible optimization

**Novelty: moderate-high. Buildability: high — LLVM already provides the
mechanism.**

Most languages take one of two bad options: optimize tail calls invisibly, so a
programmer cannot tell whether they got it and a small edit silently reintroduces
stack growth; or do not optimize them at all, so recursion is unusable for
iteration. Scheme mandates proper tail calls, which settles the *semantics* —
but in a systems language with no runtime, the fresher move is to make the
guarantee **checkable at the call site**.

LLVM's `musttail` **fails at compile time if the call is not genuinely in tail
position.** So Burxt can offer exactly the pattern it uses everywhere else:

> Declare the intent, and the compiler either guarantees it or refuses to
> compile with an explanation. Never a silent difference between "optimized"
> and "hoped for."

```text
// illustrative, not settled syntax
fn sum_to(n: Int, acc: Int) -> Int {
    if n <= 0 { return acc; }
    return tail sum_to(n - 1, acc + 1);   // constant stack, or a compile error
}
```

Why this is on-thesis rather than a borrowed convenience:

- It converts a *performance hope* into a *checked property*, which is the same
  move as rounding contracts and exhaustive matching.
- It makes deep recursion **safe** rather than usually-fine, which matters
  because stack overflow is currently the only failure Burxt does not name
  (see DESIGN.md's interim ledger).
- Immutability-by-default makes a functional style natural, and that style is
  only viable when recursion does not grow the stack.
- Self-hosting needs recursion for tree walking; a guarantee about its cost is
  worth having before the compiler is written in Burxt.

**SHIPPED in v0.0.29** — the first entry in this register to become real.
`return tail f(...)` lowers to `musttail`, measured at 50,000,000 frames in
constant stack; the same program without `tail` dies. The illustrative syntax
above is the actual syntax.

What shipped is exactly the *checkable* version argued for here: the guarantee
is explicit (never inferred, so an edit cannot silently reintroduce stack
growth), and when it cannot be honoured the compiler says why in its own words —
`musttail` requires the caller's and callee's prototypes to match, so mismatched
signatures, `extern fn` targets, aggregates travelling by hidden pointer, and
leaving a `region` are each refused with their own reason. Self- and mutual
recursion with matching signatures are the covered cases.

Still open, deliberately: this makes stack overflow **avoidable**, not **named**.
An unmarked deep recursion still dies anonymously, which is the diagnostics gap
recorded in DESIGN.md's interim ledger.

## 5. Termination as a contract

**Novelty: high (in this combination). Buildability: low — needs the verifier.**

An extension of §3 rather than a separate idea: a recursive function may carry a
**termination measure** — a `decreases` clause naming a quantity that provably
shrinks on every call, as Dafny and ACL2 do. For money code this is not
academic: an infinite loop in a payment processor is a real failure mode, and
"this function provably terminates" is exactly the class of claim the
verification layer exists to make. Pairs naturally with the conservation laws in
§3: one says the answer is right, the other says an answer arrives.

## 6. The concurrency mechanism: effect handlers, not `async`

**Novelty: moderate — real prior art, little adoption. Buildability: medium-low.**

Concurrency is wanted; **`async fn` is the wrong mechanism for it.** Colored
async splits every function into two incompatible worlds, forces a mandatory
executor (runtime baggage), and fragments across targets. Algebraic **effect
handlers** get the same capability without any of that:

- Effects are **inferred**, not written. No `async` keyword, no coloring, no
  two ways to write every function.
- The **handler**, installed by the caller, decides how an effect is
  discharged — blocking, pooled, event-loop, or mocked in a test. One function
  body serves all of them.
- Prior art exists but is not mainstream: **OCaml 5** (2022) added effect
  handlers specifically for concurrency without coloring; **Koka** does it with
  full inference. Enough precedent to be real, little enough adoption to leave
  room.

**Why this serves "native to everything" better than async:** WebAssembly is
moving toward **stack switching** (the typed-continuations proposal) precisely
to support effect handlers and green threads, and JSPI already ships in Chrome
for blocking-style wasm meeting JS promises. Effects put Burxt on the road wasm
is already taking; colored async would mean a different executor per target.

**And it unlocks §2** — forbidding effects is only possible once effects are
typed.

**Honest costs, not footnotes:**
- Effect handlers need stack capture. Native is tractable; `wasm32` today needs
  the Asyncify transform or waits for stack-switching to land.
- Effect *inference* is where these systems earn their reputation for
  impenetrable errors (both Koka and OCaml struggle here). For a language whose
  identity is "errors read as advice," that is a real fight, not a detail.
- Effects + ownership + verification is three research-adjacent systems
  interacting, built by one person. Something must be sequenced late.

**Consequence for M1, and this is the actionable part:** effect handlers capture
state across suspension points, which couples them to the memory model exactly
as async couples to it. **M1 must be decided knowing effects are the intended
concurrency mechanism**, or it will be decided in a way that fights them later.
Recorded in the far-horizon M1 entry.

---

## What is NOT claimed here

Kept explicit so the register stays honest:

- **No timeline.** Nothing here is scheduled. §1 is the nearest; §3 is furthest.
- **Not a feature list.** These are claims Burxt intends to be able to make.
  Each needs its own spec, with a must-NOT list, before any of it is built.
- **Effects are a means, not the novelty.** OCaml and Koka have them. Burxt's
  novelty is §1–§3 and §4; effects (§6) are the mechanism that makes §2
  possible.
- **Nothing here overrides the numeric core.** Exact decimals, no silent
  rounding, no float remain the foundation. Everything above extends that
  guarantee outward — none of it relaxes it.

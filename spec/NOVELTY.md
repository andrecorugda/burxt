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

The three candidates below look unrelated and are not. They are all the same
claim in different places:

> **The same inputs produce the same money, everywhere, provably — and nothing
> silently intervenes.**

Exact scaled integers give it *in memory*. Byte-identical semantics across
targets give it *across platforms*. What follows extends it across
**boundaries**, across **effects**, and across **concurrency**. That is a
coherent identity, not three features.

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

---

## 2. Provably deterministic money math (via forbidden effects)

**Novelty: high. Buildability: medium — needs an effect system first.**

If a function's effects are part of its type (see §4), they can be *forbidden*,
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

---

## 4. The concurrency mechanism: effect handlers, not `async`

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
  novelty is §1–§3; effects are the mechanism that makes §2 possible.
- **Nothing here overrides the numeric core.** Exact decimals, no silent
  rounding, no float remain the foundation. Everything above extends that
  guarantee outward — none of it relaxes it.

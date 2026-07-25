# Burxt — Far-Horizon Roadmap Specs

> Status: **direction, not implementable detail.** These four milestones are far
> enough out that writing precise specs now would be false precision — the right
> decisions depend on what the near-term milestones (control flow, grammar,
> collections) reveal. Each is captured as: the decision that dominates it, the
> honest options with tradeoffs, and what would trigger locking it down. When you
> reach one, it gets the full A4.5/A4.6 treatment (decisions-with-reasoning, must-NOT,
> ledger). This file is the arc, not the blueprint.

---

## M1 — The Memory Model (THE big fork)

**Why this is the pivotal decision in all of Burxt.** The deferred-features ledger is
already pointing here: mutating `dyn`, storable/returnable `dyn`, growable `List<T>`,
slices, string builders — they are ALL blocked on the same question: *who owns heap
memory and when is it freed.* When three or four ledger entries share one blocker,
that blocker has become the critical path. This is that blocker.

**The three honest options (no free lunch — state this plainly):**
1. **Garbage collection.** Easiest to implement and use; keeps the language
   approachable. BUT introduces pauses — non-deterministic, which fights Burxt's
   "predictable, native, no hidden costs" pillar. A GC pause mid-transaction is
   exactly what a money/systems language cannot want.
2. **Ownership + borrowing (Rust model).** No pauses, predictable, memory-safe, fast.
   BUT it is the single hardest thing to implement AND the hardest thing for users to
   learn — it directly tensions against Burxt's "easy" goal. This is the borrow
   checker, lifetimes, the whole edifice.
3. **Automatic Reference Counting (Swift model).** Middle path — predictable-ish,
   no tracing pauses, easier to learn than full ownership. BUT retain/release traffic
   has runtime cost, and reference cycles leak without extra machinery (weak refs).

**The decision that must be made, and its stakes:** this choice defines whether Burxt
is "approachable like Swift/Go" or "maximally safe/fast like Rust but with a learning
cliff." It cannot be retrofitted — it shapes the type system, the ABI, every
collection, and the whole feel of the language. It is the one decision most likely to
determine adoption, because it sets where Burxt sits on the easy↔safe axis that the
North Star names as the permanent tension.

**My honest lean (to weigh, not to bind):** given Burxt's "easy AND safe" ambition and
solo-founder reality, **ARC is the pragmatic middle** — predictable enough for the
money/systems pitch, far less implementation and learning cost than full ownership,
and Swift/Koka/Lobster prove it viable. Full ownership is the "correct" maximalist
answer but may cost Burxt its approachability and cost you years. GC is the easy
escape that undercuts the predictability pitch. But this is genuinely the hardest call
in the language and deserves its own deep, spec-first session with real code in hand —
do NOT decide it casually or early.

**Trigger to lock it down:** when the ledger has ~4+ entries blocked on ownership, or
when the first program you genuinely need cannot be written without heap allocation.
Until then, the near-term milestones deliberately stay on the safe side of the heap
boundary (see A4.4's line).

**Deferred within M1:** whichever two options you don't pick get recorded with why.

---

## M2 — FFI (the platform-API unlock)

**Why it matters:** FFI (calling C / system functions) is what lets ANY Burxt program
talk to the outside world — files, network, OS, and eventually Android/iOS/browser
APIs. Without it, cross-compiling reaches a phone that can't do anything. It is the
key that makes platform reach *meaningful*, not just possible.

**The dominant decision:** how much of the C ABI to match, and how safety is preserved
across the boundary. Options range from minimal (call a fixed set of libc functions,
as codegen already does for `printf`) to full (arbitrary C headers, struct-compatible
layouts, callbacks).

**Key constraints to honor:**
- The C ABI struct layout is a *separate* concern from Burxt's own ABI (A4.5 §6 said
  so). FFI is where C-compat layout gets built, as a translation layer, without
  disturbing Burxt's internal ABI.
- Crossing into C is inherently unsafe (C has null, no bounds checks, manual memory).
  Decision: FFI calls live behind an explicit `extern`/`unsafe`-style marker so the
  correctness guarantees are visibly suspended at the boundary, not silently.
- No implicit conversion of Burxt types to C types — explicit marshalling.

**Trigger to spec fully:** when a required program needs I/O beyond `print`, or when
starting platform work (whichever first). Likely soon after collections, since real
programs need file/network I/O.

**Minimal-first path:** start by formalizing the existing libc-call mechanism into a
declared `extern` interface for a handful of functions, before attempting general C
interop. Earn generality with a program that needs it.

---

## M3 — Cross-compilation & `--target` (platform reach)

**Why it's mostly mechanical:** the hard architectural work was done at A4.5 (target-
independent front end, LLVM backend). This milestone cashes that in. LLVM already
cross-compiles; the work is threading a target triple through and handling per-target
linking.

**The concrete work (per DESIGN.md's platform section):**
1. `burxt build --target <triple>`: thread the LLVM target triple through codegen
   instead of hardcoding the host. (inkwell already accepts a triple; codegen
   currently uses the default — this is the core change.)
2. Per-target linking: select the right linker, sysroot, and libc (glibc/Bionic/
   Apple/wasm) for the chosen target.
3. Desktop matrix FIRST (Linux/macOS/Windows) — same native path, cheapest to prove.
4. Then mobile (Android via NDK + `.so` + thin app shell; iOS via Mach-O + Xcode
   signing — the friction there is Apple's walled garden, not codegen).
5. Then web (wasm32 + JS host glue; wasm32-wasi for edge/server).

**Key constraint:** exact-decimal semantics must be byte-identical on every target —
which the scaled-integer representation already guarantees (no float = no per-CPU
divergence). This is a *selling point*: "the same money math, provably identical on
web, desktop, and mobile."

**Dominant decision:** how much of the linking/packaging story to own vs. delegate to
system tools (the `cc`/linker already used). Lean: delegate linking to system tools
per target; own only the triple selection and the object emission. Don't build a
linker.

**Trigger:** after FFI (M2) — because a cross-compiled binary that can't do I/O isn't
worth much. FFI + cross-compile together unlock real cross-platform programs.

---

## M4 — Self-Hosting (the endgame)

**What it is:** rewriting the Burxt compiler *in Burxt*, compiled by the Rust
bootstrap (stage-0) compiler. The day "Burxt compiles Burxt" is the day the language
is provably real and complete enough to build serious software — because a compiler
IS serious software.

**Why it's last:** self-hosting requires the language to be capable enough to express
a compiler — which needs (at minimum) control flow, strings, collections, structs,
enums/sum types, pattern matching, the memory model, and file I/O (via FFI). It is a
*capability certificate*, not a feature. You cannot rush to it; you arrive at it when
the language is ready, and the attempt itself reveals every remaining gap.

**The staged path (standard for every self-hosted language):**
1. Keep the Rust stage-0 compiler as the bootstrap.
2. Begin rewriting compiler pieces in Burxt (lexer first — simplest, least
   dependencies), compiling them with stage-0, checking output matches.
3. Progress through parser, typeck, codegen as the language gains the features each
   needs. Each piece rewritten is a real-world stress test that surfaces missing
   capability — self-hosting is the best language test suite there is.
4. When the whole compiler is in Burxt and can compile itself, you have a stage-1
   compiler. Compile stage-1 with stage-0, then stage-1 with stage-1; if the outputs
   match (the "fixpoint"), self-hosting is verified.
5. Stage-0 (Rust) can then be retired or kept as a reference/cross-check.

**Key decisions (defer detail):** whether to keep stage-0 alive permanently as a
trust anchor (relevant to the Thompson "trusting trust" concern), and how much of the
Rust compiler's structure to mirror vs. redesign in the Burxt rewrite.

**Trigger:** attempt a lexer rewrite once strings + collections + control flow exist —
that early partial self-host is a powerful test even before full self-hosting is
reachable. Full self-hosting waits for the memory model and enums.

---

## The dependency graph (how these order)

```
  A5.0 control flow ──┐
  A4.4 strings/coll ──┼──> M2 FFI ──> M3 cross-compile ──> (platform reach)
  A4.7 grammar ───────┘
        │
        └──> M1 memory model ──> growable collections, mutable dyn, real programs
                    │
                    └──> M4 self-hosting (also needs enums + pattern matching)
```

Near-term (A5.0, A4.4, A4.7) are largely independent and can interleave. M1 (memory
model) is the gate everything heap-related waits behind. M2→M3 is the platform track.
M4 is the certificate at the end that needs almost everything.

**The one honest meta-point:** this arc is ~years of work, and the far items (M1–M4)
will be re-specced with real precision when you reach them, because decisions made
today without the intervening code would be guesses. The value of this file is the
*shape* and the *dependency order* — so Claude Code always knows what unblocks what,
and so no milestone gets built before its prerequisites. Build near-term in the
verified-increment loop; return for a deep spec session when you hit M1, the fork that
defines what Burxt becomes.

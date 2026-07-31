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
already pointing here: mutating `dynamic`, storable/returnable `dynamic`, growable `List<T>`,
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

### AMENDMENT (2026-07-25) — the trigger has been met, and two new criteria apply

**The gate is open.** The ledger now carries at least five ownership-blocked
entries: string concatenation, mutating methods through `dynamic`, returning or
storing a `dynamic`, interpolation producing a String *value* (v0.0.17), and
growable collections. M1 is no longer premature.

Two criteria have emerged since this file was written, and **both argue against
the ARC lean recorded above.** They should be weighed before deciding:

**Criterion 1 — concurrency correctness needs ownership.** The aspiration
already in DESIGN.md ("data races as compile errors") is achievable *only* under
ownership/borrowing. ARC and GC both deliver memory safety while leaving data
races fully possible. For a money language the differentiated concurrency pitch
is "two threads cannot corrupt a balance," not "you can await many sockets" — so
choosing ARC as "the pragmatic middle" would quietly foreclose the aspiration.

Note also the separation this clarified: **memory ownership and concurrency
scheduling are different axes.** OS threads need no scheduler and no runtime
baggage under *any* M1 choice; green threads need a scheduler under *every* M1
choice. M1 does not decide whether concurrency exists — it decides whether
**sharing** is safe.

**Criterion 2 — effect handlers are the intended concurrency mechanism** (see
`spec/NOVELTY.md` §4), and they capture state across suspension points. That
couples them to the memory model exactly as async couples to it: under
ownership, state living across a suspension is the hardest case (it is what
forced `Pin` and `Send`/`Sync` on Rust); under ARC or GC it is much easier.
**M1 must therefore be decided knowing effects are the target**, not in
ignorance of it.

**The honest trilemma, stated plainly:**

| Want | Points to |
|---|---|
| Data races as compile errors (on-thesis for money) | ownership |
| Maximum approachability | ARC or GC |
| No *mandatory* runtime | rules out GC; makes ARC's atomic refcount traffic real |

Ownership + OS threads + *optional, library-level* schedulers satisfies both
pillars — no baggage, no function coloring, races caught at compile time — at
the cost of the steepest learning curve, which is precisely the easy↔safe
tension the North Star says will be managed forever.

**A reframing of "no runtime baggage" that this exposed:** it should mean **no
*mandatory* runtime**, not "no runtime ever." Burxt already applies exactly this
principle — write `dynamic` and a vtable is emitted; write none and no vtable exists
at all. Under that reading a green-thread scheduler can be a *library* that only
programs using it pay for, which keeps the pillar intact without foreclosing
concurrency.

**DECIDED (2026-07-25) — see `spec/M1-MEMORY-MODEL.md`.** The answer was
neither of the three options above: **regions, with the region as the unit of
ownership.** A region has one owner, so everything inside it is reachable by
one thread and data races are impossible by construction — WITHOUT per-object
borrow checking. Prior art: Project Verona, Pony.

This **supersedes the ARC lean recorded above.** ARC was rejected for exactly
the criterion added in this amendment: it cannot deliver data-race freedom,
which is now a must-have rather than an aspiration. GC was rejected for
pauses. Full per-object ownership was rejected as more granularity than
Burxt's transaction-shaped workloads need, at a cost in ceremony and solo
implementation time that region granularity avoids.

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
  Decision: FFI calls live behind an explicit `external`/`unsafe`-style marker so the
  correctness guarantees are visibly suspended at the boundary, not silently.
- No implicit conversion of Burxt types to C types — explicit marshalling.

**Trigger to spec fully:** when a required program needs I/O beyond `print`, or when
starting platform work (whichever first). Likely soon after collections, since real
programs need file/network I/O.

**Minimal-first path:** start by formalizing the existing libc-call mechanism into a
declared `external` interface for a handful of functions, before attempting general C
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
a compiler — which needs (at minimum) control flow, strings, collections, records,
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
        └──> M1 memory model ──> growable collections, mutable dynamic, real programs
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

---

## The production-readiness gap, measured against languages people ship in

> Added 2026-07-31, at Andre's request: *"see what other gaps we might have missed or not added
> in the roadmap to make it production ready."*
>
> **Every row was verified by running the compiler, not by reading it.** That matters: writing the
> first fixture for contract brackets found they had gone fourteen versions untested, and `it` had
> never been implemented at all despite being specified. A gap document that is wrong about a gap is
> worse than none, because it will be believed.
>
> **This is not a list of things to copy.** Several rows are DECISIONS and are marked so — no float
> is the money thesis, no bitwise was refused on purpose, no inheritance was closed in v0.0.46. A
> reader who "fixes" those has broken the language. The point is to know which absences are load
> bearing and which are merely unbuilt.

### How to read the verdict column

- **Decision** — deliberate, and the reason is on record. Not a gap.
- **Blocking** — a real program cannot be written. This is the list that loses users.
- **Papercut** — a workaround exists and is ugly.

### 1. Can you write the program at all?

| Capability | Rust | PHP / Python | Java | Burxt today | Verdict |
|---|---|---|---|---|---|
| Floating point | yes | yes | yes | **none.** `Float` exists nowhere in the compiler; `CDouble` is FFI-only | Decision, but see §5 |
| Bitwise `& \| ^ << >>` | yes | yes | yes | **none.** `&` is refused with a message pointing at `&&` | Decision — revisit, §5 |
| Integer widths, unsigned | yes | partly | yes | **`Int` only**, signed 64-bit, traps | Blocking for binary work |
| Concurrency | threads, async | threads-ish, async | threads, virtual threads | **none.** No thread, no async, no scheduler | Blocking |
| Network / sockets | std + crates | built in | built in | **nothing wrapped.** A fd is an int so it is reachable, but no library | Blocking |
| TLS / crypto | crates | built in | built in | **none**, and gated on bitwise | Blocking |
| Calling an existing C library | yes | yes (ext) | JNI | **the pointer wall** — anything returning a pointer is out of reach | Blocking |
| Dependency management | cargo | composer / pip | maven | **none.** Every program starts from `lib/` | Blocking |
| Cross-compilation | `--target` | n/a | JVM | **none.** M3 unstarted; web/Android/iOS unreachable | Blocking |
| Closures / function values | yes | yes | yes | **none.** `map`/`filter` deferred on the memory question | Papercut |
| Tuples | yes | arrays | records | **none** | Papercut |
| Char type | yes | yes | yes | **none.** A String is bytes | Decision |
| Unicode, case conversion | yes | yes | yes | **none.** No `to_upper`, no codepoints | Blocking for text |
| Regex | crate | built in | built in | **none** | Papercut |
| Reflection | limited | yes | yes | **none** | Decision |

### 2. Can you run it in production?

| Capability | Others | Burxt today | Verdict |
|---|---|---|---|
| A builtin takes a value a declared parameter would | n/a | **no.** `push` does not apply the contract widening that `storable` gives every declared position since v0.0.181 — so a `Decimal<7>` variable cannot be pushed into a `[Decimal<7, RoundHalfEven>]`, and `lib/vector.bx` cannot normalise. Builtins have their own path | Papercut, sharp — it blocked a library function |
| Reach a Decimal's unscaled integer | n/a | **no.** `as scaled` is FFI-only, so an algorithm that needs the integer representation has to route around it. `vector_magnitude` binary-searches instead, which works and is exact | Papercut |
| Set an exit code | trivial | **NOT DIRECTLY.** `external function exit` is refused — the runtime owns the symbol. A differently-named wrapper (`_exit`) works and answers 3 | **Blocking.** A CLI that cannot signal failure to a shell is not shippable |
| Write to stderr | trivial | **no.** `print` is stdout only; nothing in `lib/` reaches stderr | Blocking |
| Read an environment variable | trivial | **no.** `getenv` returns a pointer, so the wall blocks it | Blocking |
| Structured logging | crates/libs | **none.** `print` is the whole story | Blocking |
| Catch a failure / recover | `catch_unwind`, exceptions | **no.** A contract or bounds failure exits 70. There is no handler | Decision worth stating |
| Stack trace on failure | yes | **no.** The message names the clause and the function, and nothing below it | Papercut, sharp |
| Debugger / breakpoints | yes | **no DWARF.** An agent that cannot debug inserts `print`, which MOVES THE STACK and changes the answer — exactly the v0.0.141 trap | Blocking |
| Profiler | yes | **none** | Papercut |
| Test framework | built in | **none in the language.** This repo's suite is Rust + fixture directories, which a user cannot reuse | Blocking |
| Formatter | rustfmt, black | **none.** No `burxt fmt` | Papercut, but see below |
| Optimisation | flags | **O2 always** (`run_passes("default<O2>")`). No flag, no `-O0` for debugging | Fine, worth a flag later |
| Date / time | libraries | **`os_now()` — unix seconds.** No formatting, no arithmetic, no timezone | Blocking for most apps |
| Random numbers | yes | **none** | Blocking |
| Versioned dependencies / semver | yes | n/a — `use` is a path | Follows the package manager |

### 3. What Burxt has that these mostly do not

Worth keeping in view, because the gap list above is long and one-sided by construction.

| | |
|---|---|
| Exact decimals with the scale IN THE TYPE | Java has BigDecimal as a library; nobody has it as the default with mixed-scale arithmetic refused |
| A rounding rule that travels through signatures | nothing else does this |
| `burxt review` — a diff of what a program PROMISES, non-zero when weaker | no equivalent anywhere |
| Effects declared in the signature and transitive | closest is Haskell's monads, and nothing in this class of language |
| Contracts as the JSON Schema, so it cannot drift | `burxt mcp-schema`. Nothing else can, because nothing else puts the precondition in the signature |
| A byte-identical self-hosting fixpoint, checked every push | rare at any size |
| Two independent implementations that must agree | this is why five silent wrong answers this week became failing tests |

### 4. The ranking — by what fixing it unblocks

Counted rather than guessed, over §1 and §2:

1. **The pointer wall** (unblocks: every C library, sockets, TLS, env vars, platform APIs, `mmap`,
   the database idea). It is the only entry that appears in every one of Andre's three targets, and
   the prediction recorded in the plan before the count was that it would rank first. It does.
2. **Cross-compilation, M3** (unblocks: web, Android, iOS — all three reach visions at once). The
   roadmap above already says the hard architectural work is done.
3. **Bitwise + integer widths** (unblocks: binary formats, hashing, checksums, compression, a
   database file format, crypto).
4. **The small production trio — exit code, stderr, env** (unblocks: shipping *anything* as a CLI).
   Cheap, unglamorous, and currently the reason a Burxt program cannot behave like a Unix citizen.
5. **Concurrency** (unblocks: serving, using a second core).
6. **A test framework in the language** (unblocks: anyone else trusting their own Burxt code — this
   repo tests Burxt with Rust, which a user cannot do).
7. **Time and randomness** (unblocks: most ordinary applications).
8. **DWARF** (unblocks: debugging without `print`, which is the trap that costs correctness).

### 5. Two decisions this audit says should be RE-OPENED

Not overturned — re-opened, with the reason the original call may not survive contact.

**Bitwise operators.** Refused on purpose, and the refusal message is helpful. But the consequence is
that Burxt cannot parse a binary format, compute a checksum, or implement a hash — which means it
cannot write its own database file format, and Andre's local-database vision is gated on exactly
that. Reversing a stated decision needs the reason written down, and the reason is: the decision was
made when the language had no ambition to store data.

**Floating point.** The money thesis says no float, and that is right for money. But it also rules
out geometry, statistics, and standard vector similarity — which gates the RAG vision. The third
option is the interesting one and belongs in the record: **cosine similarity in `Decimal<6>` is exact
and reproducible across CPUs**, because f32 accumulation is not associative and scaled-integer
arithmetic is. No float-based vector store can claim that. So the answer may be *"vectors, exactly"*
rather than *"add floats"* — which would be on-thesis rather than beside it.

Per the plan, this decision waits for the count above rather than an argument. The count says floats
block a **narrower** set than the pointer wall does.

**Specced out in full, and it survived contact:** [N9-VECTORS-EXACTLY.md](N9-VECTORS-EXACTLY.md). The
arithmetic is verified working today — a 1536-dimension dot product at `Decimal<7>` answers exactly,
and at `Decimal<8>` it **traps**. Rows 1–5 of that spec's table need no language change at all, which
makes "the only vector store whose scores are reproducible" about a week of work rather than a
milestone. That is the strongest argument yet for keeping no-float: the flagship use nobody thought was
reachable without floats turns out to be reachable, and better, without them.

### 6. What this does NOT change

The near-term order stays: **bugs first, then usability.** Nothing in this table is a reason to stop
fixing a silent wrong answer, and five of them turned up this week while building something else.

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

**DECIDED (2026-07-25) — see `spec/1.0/M1-MEMORY-MODEL.md`.** The answer was
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

### Slice 1 — the pointer wall, opened (v0.0.196)

For one line, `external function` could return only `Int` or `CInt`, and the reason given was
*"Burxt cannot yet track who owns memory a C function returns."* That single restriction was the
largest gap in the language: every C library, every socket, every platform API and every `getenv`
sat behind it.

**What opened it was not a lifetime system.** It is a smaller idea, and worth stating as the design
rather than as an implementation note:

> **Burxt never holds the pointer as anything it can act on.**

A `CPointer` is a token. Exactly two things may be done with one —

```burxt
external function getenv(name: String) -> CPointer touches input;

function os_env(name: String) -> Option<String> touches input {
    let found: CPointer = getenv(name);
    if c_is_null(found) { return Option.None; }     // did the call fail?
    return Option.Some(c_string_at(found));         // copy the bytes out
}
```

— and there is no third. No arithmetic, no indexing, no printing, **not even `==`**. So "who frees
this" and "is it still valid later" stop being questions the compiler has to answer, because nothing
will look again. **The copy is the wall.** If C wants its memory freed, the program calls an
`external function free` itself, in the open, visible in a signature.

Every refusal has a fail fixture (`tests/fail/c_pointer_*.bx`) and a stated reason:

| refused | why, and it is not caution |
|---|---|
| `print(p)` and `"{p}"` | **an address differs between runs**, so a program printing one would not be reproducible — and reproducible output is the property everything else here protects |
| `p == q` | two pointers being equal says nothing a program can act on. The question people mean is `c_is_null(p)` |
| `p + 1` | pointer arithmetic is the operation that turns a token into a way to read arbitrary memory |
| `c_string_at(140737488355328)` | an Int is not a pointer. There is no cast, and that is the feature |
| `-> String` from an extern | a String is a Burxt value with an OWNER, so accepting one is a claim about whose memory it is that C cannot make. `-> CPointer` says the same thing and says who copied |

A null pointer **traps** in `c_string_at` rather than answering `""`: unset and empty are different
facts, and one String for both is exactly the silent wrong answer this language exists to refuse.
Byte-identical message in both compilers (`tests/panic/c_string_at_refuses_null.bx`).

**One bug found on the way, and it is the instructive kind.** Stage-1's `declared_type` maps type
node kinds 40–48 and returns UNKNOWN for anything else — and an unknown type *silences every rule
downstream*. So stage-1 accepted every `CPointer` program, including the four it was supposed to
refuse, while emitting correct code (everything is an i64 there anyway). It looked like agreement.
**A checker that agrees because it stopped asking is the failure mode the differential test exists
to catch**, and this is the shape of it: the pass fixture went green immediately and only the FAIL
fixtures revealed that nothing was being checked.

**Still closed, deliberately:**

- **Reading a C struct.** Needs integer widths (`i32`, unsigned) to describe a layout — roadmap item
  3. Until then `readdir` is reachable but `dirent.d_name` is not.
- **`mmap`'d bytes.** `c_string_at` reads to a NUL; a length-taking `c_bytes_at(p, n)` is the missing
  piece, and it wants a decision about what happens when `n` is a lie.
- **Callbacks into Burxt.** C calling Burxt is the opposite direction and a separate design.
- **An effect for the environment.** `os_env` declares `touches input`, which is honest — a value the
  process was started with — but whether reading the environment deserves its own effect is open. The
  vocabulary is closed on purpose, so adding to it is a decision and not a convenience.

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

---

### Slice 1 — `--target` and the identical-IR result (v0.0.197)

`burxt build --target <triple>` emits a real object file for that architecture. Verified by reading
each object's own header rather than trusting the exit status — `the_ir_is_the_same_for_every_target`
and `cross_compilation_emits_a_real_object_for_every_target` in `tests/runner.rs`:

| triple | container | verified by |
|---|---|---|
| `aarch64-unknown-linux-gnu` | ELF | `e_machine == 183` |
| `x86_64-unknown-linux-gnu` | ELF | `e_machine == 62` |
| `riscv64-unknown-linux-gnu` | ELF | `e_machine == 243` |
| `armv7-unknown-linux-gnueabihf` | ELF | `e_machine == 40` |
| `x86_64-apple-darwin` | Mach-O | `cputype == 0x01000007` |
| `aarch64-apple-darwin` | Mach-O | `cputype == 0x0100000C` |
| `x86_64-pc-windows-msvc` | COFF | `machine == 0x8664` |
| `wasm32-unknown-unknown` | wasm | magic `\0asm` |

**Linking is delegated, on purpose**, which is the decision this section already recorded: a foreign
link needs that target's libc, sysroot and linker, and owning that is how a compiler grows a second
job it is bad at. So a cross build emits the `.o`, keeps it, and says `not linked:` with the command
to finish it. `run --target` is refused rather than built-then-failed.

Two small things that mattered more than they look:

- The old code called `Target::initialize_native`, so **every** foreign triple failed with *"no
  available targets are compatible"* — a message about the compiler's own initialisation rather than
  about the input. Now `initialize_all`, and an unrecognised triple is named with where to look.
- A cross target gets CPU `generic` and no features. `get_host_cpu_name` would name THIS machine's
  CPU, which for a foreign triple is meaningless or wrong — and "wrong but it compiled" is the
  failure mode this language is arranged against.

#### The result that was better than predicted

This section predicted byte-identical decimal semantics across targets. What is actually true is
stronger: **the emitted LLVM IR is identical for every target above, apart from the two lines that
name the target** — and that includes the 32-bit ones. The prediction was 64-bit agreement.

Three reasons, and each is an earlier decision paying a dividend nobody designed it for:

1. **No float.** Every arithmetic operation is on an i64, so nothing depends on a rounding mode, x87
   excess precision, or fused multiply-add. The no-float thesis was argued for exact money; this is
   the same property read sideways.
2. **Layout is decided by TYPE, never by size.** An enum's payload area is counted in 8-byte cells
   from the types of its variants, so it does not move when a pointer's width does.
3. **Opaque pointers.** LLVM 15+ writes `ptr` rather than `i8*`, so pointer width never appears in
   the IR at all — which is why wasm32 and ARM32 agree with x86-64.

**What that does and does not prove.** Identical IR means nothing in the *arithmetic* can diverge, so
a `Decimal` answer is the same on every target. It does not prove identical *behaviour*: LLVM's own
lowering and the platform libc are still downstream. But that surface is far smaller than float
rounding, and it is the surface every language has. The honest form of the claim is therefore:

> The same money math, emitted identically for web, desktop and mobile — checked, not asserted.

**Still to do for a runnable cross build:** per-target linking (item 2 in this section), which needs a
sysroot per platform and is a packaging problem rather than a compiler one; and for wasm specifically,
a host story — the emitted module calls `printf` and `malloc`, so it needs either wasi or JS glue.
That is the next slice, and it is the one that turns an object into something a phone or a browser
runs.

---

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
| Network / sockets | std + crates | built in | built in | **nothing wrapped.** A fd is an int so it is reachable, but no library. **Sharpened 2026-08-01 — the row was right and vague:** the gap is exactly `sockaddr`. `socket`/`send`/`recv`/`listen`/`close` all cross the boundary TODAY (measured, §8's tier table: `socket(2,1,0)` → fd 3); only `bind`/`connect`/`accept` need a C struct. Scheduled as [M15](M15-WEB.md) W1–W2 | Blocking |
| TLS / crypto | crates | built in | built in | **none.** No longer gated on bitwise (v0.0.199) — gated on sockets and a lot of work | Blocking |
| Calling an existing C library | yes | yes (ext) | JNI | **open (v0.0.196).** `-> CPointer`, with `c_is_null` and `c_string_at`. Anything holding a handle or returning text is reachable; a C STRUCT still is not (needs widths). **Corrected 2026-08-01: this row's blocker is GONE.** Integer widths landed in **v0.0.261**, both stages, and A7's own *unblocks* column names C structs — so the layout work is unblocked rather than blocked, and is scheduled as [M15](M15-WEB.md) W1. The row went stale the moment A7 shipped and nobody came back to it, which is the failure `ROADMAP-1.1.md`'s *"NOT DONE is not evidence"* rule exists to catch | Partly |
| Dependency management | cargo | composer / pip | maven | **none.** Every program starts from `lib/` | Blocking |
| Cross-compilation | `--target` | n/a | JVM | **objects, yes (v0.0.197)** — 8 triples, ELF/Mach-O/COFF/wasm, identical IR. Linking is delegated, so a runnable foreign binary needs that target's toolchain | Partly |
| Closures / function values | yes | yes | yes | **none.** `map`/`filter` deferred on the memory question | Papercut |
| Tuples | yes | arrays | records | **none** | Papercut |
| Char type | yes | yes | yes | **none.** A String is bytes | Decision |
| Unicode, case conversion | yes | yes | yes | **none.** No `to_upper`, no codepoints | Blocking for text |
| Regex | crate | built in | built in | **none** | Papercut |
| Reflection | limited | yes | yes | **none** | Decision |

### 2. Can you run it in production?

| Capability | Others | Burxt today | Verdict |
|---|---|---|---|
| Modify an array a function was passed | `&mut` | **yes (v0.0.201).** `mutable xs: [T]` — the callee gets a pointer to the caller's storage instead of LLVM `byval`, which is what `mutable self` always did. Only aggregates; refused on a method parameter and on `pure`. This is what `lib/array.bx` was waiting for | Done |
| A builtin takes a value a declared parameter would | n/a | **yes (v0.0.194).** It was a bug, not a design: SEVEN positions still compared types with `==` while a comment claimed otherwise. `vector_normalise` shipped in v0.0.195 | Done |
| Reach a Decimal's unscaled integer | n/a | **no.** `as scaled` is FFI-only, so an algorithm that needs the integer representation has to route around it. `vector_magnitude` binary-searches instead, which works and is exact | Papercut |
| Set an exit code | trivial | **yes (v0.0.200).** `exit(code)` is a statement, not a builtin — it never returns, so it has no type to answer with. 0..=255 enforced: a literal is a compile error, a computed status traps, because POSIX hands the shell only the low eight bits and `exit(256)` would report SUCCESS | Done |
| Write to stderr | trivial | **yes (v0.0.203).** `print_error(x)` — the SAME statement as `print` with a destination flag, so the per-type formatter cannot fork. Every type and interpolation | Done |
| Read an environment variable | trivial | **yes (v0.0.196).** `os_env(name) -> Option<String>` — Option, because unset and empty are different facts | Done |
| Structured logging | crates/libs | **none.** `print` is the whole story, and it only reaches stdout | Blocking |
| Sort or order Strings | trivial | **yes (v0.0.202).** `<` on String is BYTE order — locale collation is a decision nobody wrote down, byte order needs none and is identical everywhere. `T: Ordered` includes String, so `array_sort` sorts names | Done |
| An Option-returning GENERIC | `Option<T>` | **no.** `Option.None` cannot be built where `T` is a type parameter, even written out — so `array_min<T>` takes a precondition instead (which is better, but the gap is real) | Papercut, sharp |
| A `pure` function that builds an Option | n/a | **no.** `Option.Some(x)` reads as a method call, and a method cannot be `pure` yet — so a function that only reads its arguments is refused the word saying so | Papercut |
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

1. ~~**The pointer wall**~~ — **DONE (v0.0.196).** The count said it would rank first because it is
   the only entry appearing in all three of Andre's targets, and the prediction recorded before the
   count was the same. See §M2 below for what it turned out to cost, which was far less than the
   ranking implied: no lifetime system, no ownership tracking, one new type and two builtins.

   What it opened immediately: `getenv` (so `lib/os.bx` has `os_env` — one third of item 4 below),
   and any C API that hands back a handle or text. What it did NOT open: reading a C **struct**,
   which needs integer widths (item 3), and `mmap`'d bytes, which needs a length-taking read.
2. **Cross-compilation, M3** — **object emission DONE (v0.0.197)**, for ELF/Mach-O/COFF/wasm across
   eight triples, and the IR turned out to be identical on all of them rather than merely equivalent.
   What remains is per-target LINKING (a sysroot-and-packaging problem, not a compiler one) and a
   wasm host story. See §M3 Slice 1.
3. **Bitwise + integer widths** — **bitwise DONE (v0.0.199)**, as seven named builtins plus hex
   literals, with CRC-32 checked against the standard's published values. Integer widths (`i32`,
   unsigned) remain, and they are what a C *struct* layout and a fixed-width record need. See §5.
4. ~~**The small production trio — exit code, stderr, env**~~ — **DONE.** `os_env` (v0.0.196),
   `exit(code)` (v0.0.200), `print_error(x)` (v0.0.203). A Burxt program can now behave like a Unix
   citizen: read its configuration, say what went wrong on the right stream, and tell the shell.

   stderr was built as ONE statement with a destination rather than two statements, and that decision
   paid for itself inside the same version: stage-1's Decimal path was writing its digits with its own
   `printf` and only the newline through the shared helper, so `print_error($19.99)` split across both
   streams. One formatter with one exit made that a one-line fix instead of a divergence to hunt.
5. **Concurrency** (unblocks: serving, using a second core).
6. ~~**A test framework in the language**~~ — **DONE (v0.0.204).** `lib/test.bx`: a tally threaded
   through a `mutable` parameter, per-type checks that report the values, failures on stderr, and a
   status a build can fail on. It could not have been written a week earlier — `mutable` parameters
   (v0.0.201) are what let a tally be threaded, and `exit(code)` (v0.0.200) is what lets a suite fail
   a build.

   **What a Burxt test can do that others cannot: it asserts a VALUE, not a range.** There is no
   `check_close` and there will not be one — no float means no last-digit wobble, so `$59.97` is
   `$59.97` on every machine, target and run. Elsewhere a money test either compares with a tolerance,
   which hides the bug it was written to catch, or is flaky.
7. **Time and randomness** (unblocks: most ordinary applications).
8. **DWARF** (unblocks: debugging without `print`, which is the trap that costs correctness).

### 5. Two decisions this audit says should be RE-OPENED

Not overturned — re-opened, with the reason the original call may not survive contact.

**Bitwise operators — REVERSED (v0.0.199).** Refused on purpose, and the refusal message was helpful.
But the consequence was that Burxt could not parse a binary format, compute a checksum, or implement a
hash — so it could not write its own database file format, and the local-database vision is gated on
exactly that. **The reason for the reversal, on record: the decision was made when the language had no
ambition to store data.**

What landed is not `&`. Seven **named** builtins:

```burxt
bit_and(a, b)   bit_or(a, b)   bit_xor(a, b)   bit_not(a)
shift_left(x, n)   shift_right_zeros(x, n)   shift_right_sign(x, n)
```

**Names rather than operators, and this is the same call `divide_floor` already records rather than a
new one:**

1. `a & b == c` means `a & (b == c)` in C, and has been a bug in every C program that forgot. This
   language's claim is that a reviewer can *see* a program is right; a precedence table they have to
   remember is the opposite of that. `bit_and(a, b) == c` cannot be misread.
2. **The right shift is genuinely two operations.** On a negative value, filling with zeros and copying
   the sign bit give different answers, and one symbol cannot say which — exactly the situation `/` on
   two Ints is in. Once the shift needs two names, giving `&` an operator would be the inconsistency.

So `&` and `|` keep their lexer error, now naming the replacement, in both compilers.

**A shift distance outside 0..=63 is refused, not answered.** This is the part that is about the
thesis rather than about completeness: a shift by 64 is *undefined* in LLVM, and on x86 the hardware
masks the distance to six bits — so `x << 64` silently answers `x`. A literal is a compile error; a
computed distance traps at runtime, byte-identically in both compilers.

**`shift_left` discards bits past the top, and is the one place in the language where losing
information is not an error** — because that is what a shift is *for*. It is therefore explicitly not
`x * 2^n`: multiplication traps on overflow and this does not. Stated in the name's documentation so
nobody reaches for it as a fast multiply.

**Wrapping addition was NOT added**, and the reason is the same shape: `+` traps, correctly, because a
total that silently wraps is the wrong answer. But a checksum needs the wrap — and the wrap is
*constructible* from bit operations (`tests/pass/bits.bx` builds it with half-adder logic). Writing it
out makes the carry being discarded visible; `wrapping_add(a, b)` would require the reader to know what
the name promises.

**Hexadecimal literals came with them**, for their sake: a mask, a CRC polynomial or a protocol field
is written in hex wherever it is *specified*, so a reviewer checking `0xEDB88320` against the standard
that defines it compares the same characters. `3988292384` is the same number and a worse review.
`0xFFFF_FFFF` groups digits; sixteen digits fill a signed Int, so `0xFFFFFFFFFFFFFFFF` is `-1` and
equals `bit_not(0)`; a seventeenth is refused. No hex Decimal — a scale counts *decimal* places.

**The proof is a real checksum, not a demonstration.** `tests/pass/bits.bx` computes CRC-32 and checks
it against the three values the standard publishes for exactly this purpose:

| input | expected | |
|---|---|---|
| `"123456789"` | `0xCBF43926` | the standard's own check value |
| `"a"` | `0xE8B7BE43` | |
| `"The quick brown fox jumps over the lazy dog"` | `0x414FA339` | |

Byte-identical in both compilers. And the nicest confirmation that the feature was needed: **stage-1's
own hex lexer uses it.** Accumulating `hex * 16 + d` traps on the sixteenth digit, because
multiplication does not wrap — so the lexer that reads hex literals is written with the shift and or
that the literals exist to serve.

**Still not done, and separable:** integer widths (`i32`, unsigned). Those are what a C *struct* layout
needs, and what a fixed-width record format wants. Bit operations alone are enough to pack and unpack
bytes by hand, which is most of a binary format — so this unblocks N9 row 6 without them.

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

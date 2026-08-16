# Burxt design record — grouped by the version each decision shipped in

**This is the record, not the plan.** Each file is written the same way: decisions with reasoning,
an explicit **must NOT do** section, and a deferred ledger with the trigger that would earn each
deferral a future milestone. They are kept because the REASONING is the valuable part — the boundary
a package draws was decided by re-reading `M6-MODULES.md` §5 on the day it was needed, four hundred
versions after it was written.

## The layout

| where | what |
|---|---|
| **[`1.0/`](1.0/)** | the twenty-three specs that built **1.0.0**, and [`1.0/ROADMAP-1.0.md`](1.0/ROADMAP-1.0.md), the record of how it was built row by row |
| **`spec/`** (here) | what is still LIVE: the standing rules, the ambition, and what is planned next |

**Grouping by version rather than deleting.** A shipped spec is not waste — it is why a decision is
the way it is, and this project has spent whole afternoons recovering reasoning that was written
down and then not found. What the grouping fixes is the other failure: a reader landing on a folder
of thirty "plans" cannot tell which describe a language that exists.

## What is live

| file | what it is |
|---|---|
| [`A7.0-NAMING.md`](A7.0-NAMING.md) | the naming rules, **in force** and tested — not version-scoped |
| [`NOVELTY.md`](NOVELTY.md) | **ambition**: what Burxt exists to solve that other languages do not |
| [`FAR-HORIZON-ROADMAP.md`](FAR-HORIZON-ROADMAP.md) | direction only, deliberately not a plan |
| [`N9-VECTORS-EXACTLY.md`](N9-VECTORS-EXACTLY.md) | rows 1–5 shipped; 6–9 are open |
| [`A8.0-RECORD-UPDATE.md`](A8.0-RECORD-UPDATE.md) | specified, **undecided** — Andre's call |
| [`M16-NETWORK.md`](1.1/M16-NETWORK.md) | **sockets and processes, DONE** after 1.0.0 — and the wall that was one builtin |
| [`ROADMAP-2.0.md`](ROADMAP-2.0.md) · [`M15-WEB.md`](M15-WEB.md) | hosts and the web half — **now 2.0**, renamed once 1.1.0 shipped without them |

**Two disciplines live here and must not be confused.** The milestone specs govern
**implementation** — what gets built, and the scope rules that keep it sound. `NOVELTY.md` holds
**ambition**. Scope discipline applies to the former only; applied to the latter it just produces
incremental answers.

## The audit below

Every row was checked by RUNNING the compiler rather than by reading the spec, and the status column
is the authority — nine spec headers claimed "to implement" for shipped work until v0.0.295, and the
index was right every time. Where a spec and the implementation disagree, the note says so.

## Status at a glance

| Spec | State | What remains |
|---|---|---|
| [A4.4 Strings & Collections](1.0/A4.4-STRINGS-COLLECTIONS.md) | **DONE bar one view** | Arrays fixed (v0.0.10) and growable (v0.0.24). Strings: literals, printing, FFI, length, equality, `byte_at` (v0.0.21), **concatenation (v0.0.25)**, **`read_file` / `to_string` (v0.0.28)**. Remaining: `.chars()`. |
| [A4.5 Aggregate ABI](1.0/A4.5-AGGREGATE-ABI.md) | **DONE** (v0.0.12) | — |
| [A4.6 Interfaces & Dispatch](1.0/A4.6-INTERFACES-DISPATCH.md) | **DONE and CLOSED** (v0.0.14) | Interfaces, `implement`, static + `dynamic` dispatch. `class` / `open` inheritance was **dropped in v0.0.46** — nothing needed it across thirty versions, so composition-only is final. |
| [A4.7 Signature Grammar](1.0/A4.7-SIGNATURE-GRAMMAR.md) | **Mostly done** (v0.0.17–v0.0.19, v0.0.28) | Brace hazard, interpolation (as a print, then as a value), money and percent literals, mixed-scale `*` all shipped. Remaining: unit literals (`5.km`), `requires`/`ensures`, pipelines. |
| [A5.0 Control Flow](1.0/A5.0-CONTROL-FLOW.md) | **DONE** (v0.0.3–v0.0.4, v0.0.15) | — |
| [Far-horizon M1–M4](FAR-HORIZON-ROADMAP.md) | **Direction only** | Re-spec each on arrival. **M1's trigger is now MET** — see its amendment; two new criteria argue against the ARC lean. |
| [A6.0 Sum Types](1.0/A6.0-SUM-TYPES.md) | **DONE** (v0.0.20) | Enums, exhaustive `match`. Deferred: wildcards, recursive/aggregate payloads (M1), guards, nested patterns, match-as-expression, generics. |
| [A7.0 One word per concept](A7.0-NAMING.md) | **In force, and tested** (2026-07-29) | The naming rule: a concept gets **one word**, everywhere it appears — `find_<thing>` / `<thing>s` / `<Thing>`. Raised by Andre asking whether the lookup was `find_sym` or `find_bind`; the audit's finding was that **the convention already existed** and four of six lookup families followed it, but nobody had written it down, so the other two drifted and nothing noticed. `one_word_per_concept_in_the_burxt_compiler` in `tests/runner.rs` enforces the mechanically checkable part. **This row was missing until 2026-08-01** — the spec existed, unlinked, for three days, and it was the new `every_spec_is_linked_from_its_index` invariant that found it, on its first run |
| [A8.0 Record update & class ergonomics](A8.0-RECORD-UPDATE.md) | **spec, to implement** | `self with { field: value }`. Raised by Andre asking why a method re-lists every field to change one. The argument is **correctness, not brevity**: a transposed copy of two same-typed fields compiles with no diagnostic and answers wrong — measured at v0.0.230 — and the hand-copy is the only place in the language where that is both possible and invisible. States its own cost: `with` makes "unchanged" implicit, and a class invariant is what pays for it. |
| [M1a Caller-Region Functions](1.0/M1a-CALLER-REGION-FUNCTIONS.md) | **DONE** (v0.0.38) | `allocates` on a signature: build in the caller's region, return what you built. Deferred: `allocates` on methods. |
| [M1 Memory Model](1.0/M1-MEMORY-MODEL.md) | **DONE** (v0.0.24–v0.0.27) | All four slices shipped: regions + bump allocator, growable arrays with escape checking, string concatenation, storable `dynamic`. Two of its predictions were corrected rather than forced — see §6a. |
| [**Roadmap to 1.0.0**](1.0/ROADMAP-1.0.md) | **THE PLAN OF RECORD** (2026-07-31) | The goal — *a language someone outside this repository can ship on* — and the order of work: **A** compiler fixes by leverage, **B** urgent bugs, **C** the rest of the bar, **D** full Rust `str`+`Vec` parity in the library, **E** security build-vs-bind, then post-1.0 by gate. Built from all 26 specs plus three scans of the compiler and library against Rust/Python/PHP/Java/Go. Supersedes FAR-HORIZON's §4 ranking for near-term work; that document remains the audit |
| [**Roadmap 2.0 — hosts, and the web stack**](ROADMAP-2.0.md) | **THE PLAN OF RECORD for 2.0** (v0.0.260; Part II added 2026-08-01) | **Two unrelated halves, in one file so that a reader asking "what is in 1.1" finds all of it.** **Part I — hosts**, split from 1.0 by **verifiability**: the distribution work that *cannot be finished by writing it*, because finishing means proving it on hardware nobody here has. Android as a **host** (an experiment with the command written down — NDK r27 *is* LLVM 18, so the version objection is gone), the native Windows port with its bill itemised and refused, and the `use`-search-path question the container raised. Opens by separating **target** from **host**, because "supports Android" means one of two things and only one is hard — Burxt has emitted for Android since v0.0.197. **Part II — the web stack**, split from 1.0 by Andre's call that the core comes first; the detail is in [M15](M15-WEB.md) and the summary is there |
| [Production-readiness gap](FAR-HORIZON-ROADMAP.md#the-production-readiness-gap-measured-against-languages-people-ship-in) | **Audited 2026-07-31** | What a user cannot do, against Rust / PHP / Python / Java, every row verified by running the compiler. Ranked by what fixing it unblocks — the **pointer wall** comes first, and it was predicted to before the count. Marks which absences are DECISIONS (no float, no bitwise, no inheritance) so nobody "fixes" the identity, and re-opens two of them with the reason the original call may not survive contact |
| [N9 Vectors, exactly](N9-VECTORS-EXACTLY.md) | **Specified; the ARITHMETIC verified working** | The same query returns byte-identical scores on every CPU, and the compiler traps rather than silently losing a digit — because f32 addition is not associative and scaled-integer addition is. Cosine needs no square root if vectors are stored normalised, and inner product and squared-L2 need none at all. `Decimal<7>` is the sweet spot: exact at 1536 dimensions, and scale 8 **overflows**, measured. Rows 1–5 of its table need NO language change |
| [Novelty register](NOVELTY.md) | **Ambition — §4, §1, §2, §3 and §5 (runtime forms) shipped** | What Burxt is *for*: exactness across boundaries, provable determinism, conservation-law contracts, effects-not-async. §4 guaranteed tail calls shipped in v0.0.29; §1's FFI half in v0.0.30. |
| [M4 Self-Hosting](1.0/M4-SELF-HOSTING.md) | **Phases 1–4a DONE** (v0.0.51–57) — stage-1 parses and typechecks itself | The staging, with measured sizes: ~10–12.5k lines of Burxt for stage-1, a textual-LLVM-IR backend, six phases, and the public milestone at the end of phase 4. |
| [N5 Termination](1.0/N5-TERMINATION.md) | **Slice 1 DONE** (v0.0.45) | `decreases`, checked at every recursive call site, so it works with `return tail`. Deferred: mutual recursion, methods, lexicographic measures, static proof. |
| [A5 Contracts](1.0/A5-CONTRACTS.md) | **DONE** (v0.0.43–v0.0.44) | `requires` / `ensures` runtime-checked with the clause quoted on failure, clauses must be pure, contracts on methods, and `old(...)` — so NOVELTY §3's conservation laws are checkable. Deferred: static proof, `old` of an aggregate, derived mutual exclusion (needs threads). |
| [N2 Pure Functions](1.0/N2-PURE-FUNCTIONS.md) | **Slice 1 DONE** (v0.0.39) | `pure function`: no I/O, no FFI, no impure calls. Deferred: pure methods, purity as a parameter requirement, purity-driven optimisation. |
| [N1 Boundary Exactness](1.0/N1-BOUNDARY-EXACTNESS.md) | **Slice 1 DONE** (v0.0.30) | `CDouble`, `as scaled` marshallers, range-checked `Int` → `CDouble`, linker pass-through. Remaining: serialization and database boundaries, once an encoder exists to guard. |

### Newer milestones

| Spec | State | What it is |
|---|---|---|
| [M5 (in M4)](1.0/M4-SELF-HOSTING.md) | **DONE** (v0.0.79–v0.0.80) | The Burxt backend compiles all 88 pass programs, and the suite runs on Burxt |
| [M6 Modules](1.0/M6-MODULES.md) | **DONE** (v0.0.81–v0.0.82) | `use "path"`, one buffer with a source map, and the compiler split into five files |
| [M7 Generics](1.0/M7-GENERICS.md) | **Slices 1–3 DONE in stage-0** (v0.0.93–96) | Generic functions, enums and bounds (`Ordered`, `Equatable`, any trait — statically dispatched). Remaining: generic records, stage-1 |
| [M8 Errors](1.0/M8-ERRORS.md) | **DONE** (v0.0.94, 97) | `Option<T>` and `Result<T, E>` in `lib/`, written in Burxt. `?` recognises the failure by VARIANT name, so neither type is known to the compiler — and it does not convert between error types |
| [M9 Performance](1.0/M9-PERFORMANCE.md) | **DONE** (v0.0.87–v0.0.90) | The self-compile: 190 s → 1.17 s, ~1 GB → 196 MB. `byte_at` bounds-checked with a `strlen` per byte |
| [M10 Ergonomics](1.0/M10-ERGONOMICS.md) | **Slices 1–2c DONE** (v0.0.91–92, 95, 98) | `let x = 0;`, `for x in xs`, trailing commas everywhere, `function (self)` inside an `implement` — both compilers. Plus the rounding rule corrected: a contract where a value narrows, and nowhere else |
| [M11 Maps](1.0/M11-MAPS.md) | **DONE** (v0.0.119) | `Map<K, V>` in `lib/map.bx` — ordinary Burxt, one builtin (`hash`). Iteration order is INSERTION order, defined rather than unspecified, because a container whose order depends on a hash function is a determinism hazard in a language whose thesis is reproducibility. `find` answers `Option<V>` since v0.0.118 |
| [M12 Strings](1.0/M12-STRINGS.md) | **DONE** (v0.0.121) | Both compilers, fixpoint intact. Known gaps tracked elsewhere: the `string_split` separator is a single BYTE, so `", "` cannot be split on, and there is no case conversion |
| [M13 Contract syntax](1.0/M13-CONTRACT-SYNTAX.md) | **DONE, both compilers** (v0.0.135; `it` v0.0.167; stage-1 v0.0.169) | `function f(p: Type [> 0, <= q]) -> T [>= 0]` — the claim on the value it constrains. The comma is AND, `||` is OR. Desugars to `requires`/`ensures` with the SUBJECT synthesized into the message: `p > 0`, never `> 0`. Andre's call, v0.0.166. Stage-1's wall — the return bracket's `result` is written in no program — dissolved into a NODE KIND with no name: a thing that has no name needs no way to spell one |
| [M14 Implicit regions](1.0/M14-IMPLICIT-REGIONS.md) | **Slices 1–2 DONE** (v0.0.142–146) | `allocates` inferred rather than written, and allocation outside a `region` is no longer an error — so a five-line program is five lines. Slice 3, per-block RELEASE, is open and is the memory half: without a region, memory grows in a straight line (5,280 KB against 1,408 KB per 100k Strings) |
| [M15 Web](M15-WEB.md) | **SPECIFIED, nothing built — scheduled for 2.0** (2026-08-01); its socket foundation shipped early in 1.1.0 | The web stack as **primitives, not a framework**: `html.bx`, `cgi.bx`, `net.bx`, `http.bx`. Andre's call — 1.0 is the real core and comes first, and the goal is that someone else builds the framework, because that is how a language acquires an ecosystem. **The finding that reorders it: W0 is gated on nothing.** The typed `Html` tree — `Text`/`Raw`/`Element`, escaped on render so an unescaped value is unrepresentable — was **measured compiling and running on today's compiler**, and `cgi.bx` needs only `os_env` and `os_read_all`, which exist. So a Burxt binary behind nginx serves dynamic pages with no socket and no concurrency. Only the listener waits: `sockaddr` needs C struct layouts (unblocked by A7 v0.0.261), and a server needs §G1, where a serial loop and `fork()` were both **refused** in favour of `NOVELTY.md` §6's effect handlers |

## The audit, in detail

Everything below was checked by compiling and running a probe program, not by
reading code.

### A5.0 Control Flow — DONE

Built long ago and out of the specified order: `Bool` with `true`/`false`, all
six comparison operators with the same-type rule, `if` / `else if` / `else`,
`while`, block scoping, and early `return` all shipped in v0.0.3–v0.0.4.

The spec's own acceptance program passes — **`fib(10)` prints `55`, `fib(20)`
prints `6765`** — so by §6's criterion, "the language can express algorithms"
is already true.

The last gap — `&&`, `||`, `!` (deliverable 3) — closed in **v0.0.15**, with
short-circuit built as real basic blocks and proven observable by two tests
(a skipped side effect, and a division by zero that never executes). `&` and
`|` alone are errors pointing at the doubled forms.

Two deviations from the spec worth recording, both deliberate:

- **`Bool` is an i64 holding 0/1, not an LLVM `i1`** (spec §4). One uniform
  value width keeps variables, parameters, and returns simple; `i1` appears
  only transiently at comparisons and branches. No observable difference.
- ~~`break` / `continue`~~ — **shipped in v0.0.50**, earned by three self-hosted
  programs each working around their absence.

### A4.4 Strings & Collections — arrays done, strings half done

**Arrays are complete** (v0.0.10) and match the spec: `[T; N]`, literal
construction, bounds-checked reads, element assignment through a `let mutable`
binding, compile-time rejection of a constant out-of-range index, and a
runtime trap naming the index and valid range. `len(a)` is constant-folded.
Not built: the `[0; N]` repeat form (sugar, deferred).

**Strings are only half done** (v0.0.7). Literals with the four escapes,
printing, immutability, and passing to C as `const char*` all work. Missing:

- ~~**Length**~~ and ~~**equality**~~ — **shipped in v0.0.16** as generated
  byte-scan helpers, exactly as this audit predicted. `==` slots into the one
  equality rule; comparison is by bytes, not pointers.
- ~~**`.bytes()`**~~ — **shipped in v0.0.21** as `byte_at(s, i)`, named for
  bytes so the byte-vs-char ambiguity cannot hide. Bare `s[i]` stays correctly
  absent, per the spec's byte-vs-char decision. **`.chars()` is the one
  remaining gap in A4.4.**
- ~~**Concatenation**~~ — **shipped in v0.0.25**, once regions existed. It was
  correctly refused before that: it needs allocation, which was M1's job.
- **Beyond the spec, because self-hosting needed it (v0.0.28):**
  `read_file(path)` reads a file into the current region, and `to_string(v)`
  renders Int/Bool/Decimal into one. A compiler that cannot read its input or
  build an error message is not a compiler.

**The important audit finding:** the spec bundles these four together, but
they are not equally blocked. Length and equality need **no heap at all** — a
length is a byte scan or a stored count, and equality is a `memcmp`-style loop
returning a `Bool`. Only concatenation is genuinely heap-blocked. So the
string half of A4.4 can be advanced now, and only concat waits for M1. Worth
correcting because the spec's framing ("what waits for the memory model")
would otherwise defer more than necessary.

### A4.7 Signature Grammar — not started, and one hazard

None of the six deliverables exist. `$19.99` fails at the lexer; `requires` /
`ensures` are not keywords; `|>` does not parse.

**The hazard below was fixed as specified, in v0.0.17:** a bare `{` in a string
literal is now a compile error demanding `\{`, so no existing program changed
meaning silently. Interpolation works in `print` (v0.0.17, no allocation) and as
a String value (v0.0.28, desugared to `to_string` + `+`). Kept for the record:

**Hazard to fix before interpolation ships:** `print("hi {name}")` compiles
today and prints `hi {name}` **literally**. Braces are ordinary characters in
a string literal right now, so introducing interpolation *changes the meaning
of existing valid programs* — silently, which is precisely what Burxt refuses
elsewhere. When interpolation lands, `{` in a literal must either become
interpolation or be a compile error demanding an escape (`\{`). It must not
stay ambiguous.

**The inference note below still stands** — `$19.99` shipped in v0.0.18 with
its type taken from the annotation, so local inference was deliberately NOT
smuggled in. Kept for the record:

**Note on `$19.99` and inference:** `let price = $19.99;` requires inferring a
binding's type from its initializer. Every `let` in Burxt currently *demands*
an explicit type annotation. So this deliverable quietly introduces local type
inference — a real language change, not just a literal form. It deserves to be
called out as its own decision rather than smuggled in as sugar.

### A4.5 / A4.6 — done, with one ABI correction discovered by building them

Both were implemented directly from their specs and hold their guarantees:
layout is exactly the declared fields with no hidden header (machine-checked),
aggregates pass `byval` and return via `sret`, static dispatch emits no
vtable, and a record's field offsets are byte-identical with and without
`dynamic`.

One correction the A4.6 work forced on A4.5: **method receivers pass as a
plain pointer, never `byval`**. A vtable slot cannot name a concrete type, so
it cannot carry `byval(T)`, and mixing the two lowerings made a direct call
and an indirect call disagree about the ABI — producing silently wrong values.
Recorded in DESIGN.md's interim ledger.

## What is next, and why

In dependency order, cheapest and most-unblocking first:

1. ~~Finish A5.0: `&&`, `||`, `!` with short-circuit.~~ **Done in v0.0.15**,
   with short-circuit proven observable by two tests.
2. ~~Advance A4.4's strings: length and equality.~~ **Done in v0.0.16.**
   Equality landed inside the existing one-equality rule rather than beside
   it, which was the point of doing it while String was still small.
3. ~~A4.7 Signature Grammar.~~ **Mostly done** (v0.0.17–v0.0.19): the brace
   hazard, interpolation, money and percent literals, and mixed-scale
   multiplication all shipped. Remaining and unblocked: unit literals
   (`5.km`), `requires`/`ensures` as runtime-checked grammar, pipelines.
4. ~~A6.0 sum types + exhaustive matching.~~ **Done in v0.0.20.**
5. ~~String byte access.~~ **Done in v0.0.21** as `byte_at(s, i)`, named for
   bytes so the byte-vs-char ambiguity cannot hide.
6. ~~The partial self-host: a Burxt lexer in Burxt.~~ **Done in v0.0.21** —
   `examples/lexer.bx`, heap-free via spans and arithmetic accumulation.
7. ~~The parser is M1-blocked.~~ **That was wrong** — corrected in v0.0.22.
   An arena AST (children by index, not pointer) needs no recursive types and
   no heap, so `examples/parser.bx` parses and evaluates in Burxt today. It
   needed three conservative restrictions lifted, none semantic.
8. ~~Growable storage — genuinely M1-shaped.~~ **Done in v0.0.24–v0.0.27.**
   Regions, growable `[T]` with escape checking, concatenation, storable `dynamic`.
   `examples/parser.bx` declares no node budget at all: 599 nodes on a 300-term
   expression, all released as a unit.
9. ~~The compiler cannot read its own input, or report on what it read.~~
   **Done in v0.0.28**: `read_file` + `to_string`, which also retired
   interpolation-as-a-value, the oldest entry on the ledger.
10. ~~Guaranteed tail calls (NOVELTY §4).~~ **Done in v0.0.29** as
    `return tail f(...)` → `musttail`: 50M frames in constant stack, refused
    with a reason whenever the guarantee cannot be given. The first entry in the
    novelty register to ship. It also surfaced two region bugs — an early
    `return` leaked the region, and the return-path prover did not know a region
    body could return at all.
11. ~~NOVELTY §1, exactness that survives the boundary.~~ **Slice 1 done in
    v0.0.30** — the FFI half, which is the only boundary that exists today.
    `spec/1.0/N1-BOUNDARY-EXACTNESS.md` records the decisions; the serialization half
    waits for an encoder to guard. Linker pass-through shipped with it, because
    an `external function` is only half an FFI.
12. ~~Editor support: highlighting, and a language GitHub can recognise.~~
    **Done in v0.0.31** — TextMate grammar + VS Code extension + `burxt check`,
    with the grammar locked to the compiler's keyword table by a test. See
    `editors/README.md`.
13. ~~Source spans, then machine-readable diagnostics, then `burxt lsp`.~~
    **Spans and diagnostics done in v0.0.32** — caret rendering in the terminal,
    `--json` with LSP positions for editors, and a test asserting every rejection
    points at real code (it found five position-less errors on its first run).
    **`burxt lsp` shipped in v0.0.33**: diagnostics on change in any LSP-speaking
    editor, with a hand-written JSON layer so the compiler keeps its single
    dependency. **VS Code got live diagnostics in v0.0.34** without any npm dependency, by
    feeding the buffer to `burxt check - --json`. **Expression spans and hover shipped in
    v0.0.35**, which also sharpened every caret to the sub-expression. Remaining
    editor work: go-to-definition, error recovery (more than one error at a time),
    and a tree-sitter grammar for Neovim/Helix colour. **VS Code moved onto the
    language server in v0.0.36**, so hover works there too, still with no npm
    dependency. **Error recovery shipped in v0.0.37**: every type error is
    reported at once, cascade-free because every `let` declares its type.
14. Still unblocked polish: A4.7's units/contracts/pipelines, `.chars()`,
    `[0; N]` repeat literals, iterative AST walkers, and a
    NAMED stack-overflow error (tail calls avoid it; they do not name it).

**Priority note:** the stated goal is a compiler written in Burxt. Measured
against that, A4.7's leftovers (units, contracts, pipelines) add eloquence and
verification but no capability, so they rank below string bytes and the
self-host attempt.

M1 (the memory model) is **decided and shipped** — regions as the unit of
ownership, so data races are unrepresentable without per-object borrow checking.
What remains ownership-shaped is not memory: returning a `dynamic` needs borrow
tracking, and mutating through one needs mutability tracking. Both were
re-diagnosed in v0.0.26 rather than left on the memory ledger.

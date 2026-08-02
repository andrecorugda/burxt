# Burxt 1.0.0 — the road to a language people can ship on

> Status: **the plan of record.** Written 2026-07-31 at v0.0.205, from a full read of all 26 specs plus
> `DESIGN.md` and three systematic scans of the compiler and standard library against Rust, Python,
> PHP, Java and Go. Every claim here was verified by **running** the compiler, not by reading it.
>
> This supersedes `FAR-HORIZON-ROADMAP.md`'s §4 ranking for near-term work. That document remains the
> **audit** — the row-by-row comparison and the record of which absences are decisions. This one is the
> **order**.

## The goal

> **Burxt 1.0.0 — a language someone outside this repository can ship on.**
>
> They can write a real program, **test** it, **debug** it, **depend** on other people's code, and hand
> it to a shell that understands what it says. Nothing basic is missing compared to Rust, Python, PHP,
> Java or Go — and everything deliberately absent is **named, with its reason**, in one place.

Four things were chosen as must-ship, and they define the bar: **dependency management**,
**DWARF/debugger**, **time + date + randomness**, **integer widths + Unicode**. Everything not on that
list becomes a **documented limitation in the release notes**, never a silent gap.

The standard-library bar was set separately and it is the harder one: **full parity with Rust's `str`
and `Vec`**, not the common 80%. That choice is what promotes closures, tuples and an iterator protocol
from "nice later" into 1.0 — see §A.

## THE GATE — v0.0.218, and it outranks everything below it

> **Andre:** *"new roadmap rule: nothing will move until burxt compiler can equal rust compiler. Do all
> necessary, if needed create task as priority to solve all necessary issue for burxt compiler. I will
> not allow that burxt is using rust — we use rust to build burxt."*

**Nothing in §A through §H moves until the Burxt compiler equals the Rust one.** Not the twelve compiler
fixes, not the urgent bugs, not the standard-library floor, not DWARF or dependencies. The gate is above
all of them, including above the ordering rule that used to be first.

**The distinction the rule turns on**, because it is what makes it a principle rather than a preference:

> **Rust may BUILD Burxt. Burxt may not USE Rust.** A bootstrap is a one-time debt — someone had to
> write the first compiler in something. A *dependency* is permanent: if `burxt review` only exists in
> Rust, then Burxt cannot enforce its own compatibility promise without Rust; if `mcp-schema` only
> exists in Rust, the one capability no other language has belongs to the Rust program. Every tool that
> lives only on the Rust side is a claim Burxt cannot make on its own.

That is why `spec/M4-SELF-HOSTING.md` §5's *"stage-0 is the trust anchor and the differential test"*
survives the rule intact and is not in tension with it. **A differential test is not a dependency.** Two
implementations that must agree is how a language change becomes a failing test instead of a bug report,
and keeping the Rust one for that is deliberate. What the rule forbids is Burxt *needing* it to do a job.

**Where the gate stands, measured** (`every_rust_module_has_a_burxt_counterpart_or_a_reason`):

| | Count |
|---|---|
| Rust modules answered by a Burxt counterpart | **9 of 11** |
| **Held byte-for-byte by a test** | **2** — `diag.bx`, `schema.bx` |
| Written but NOT verified | 2 — `review.bx`, `lsp.bx` |
| Still Rust-only | **0** |

**`answered` deliberately excludes the two unverified ones**, and that needed a fourth strength
level in the map. `review.bx` reached the tree by accident in v0.0.220 — a subagent recreated it
between my set-aside and my `git add -A` — so the committed tree failed its own orphan check and CI
went red. The two tempting fixes were both wrong: delete a colleague's 49 KB of real work, or map it
as though it were verified. **A module nobody has compared is not parity**, and letting it raise the
number would be the exact self-deception this map exists to prevent.

### The bar, corrected in v0.0.234 — it was WRONG, not merely unmet

This section said: **"the gate is met at 11 answered AND 11 verified."** I set that two hours before
discovering it is the wrong measure for two of the eleven rows, and lowering it quietly would have
been the fourth time in one day that I moved a number instead of fixing an instrument.

**`codegen.rs` against `emit.bx` cannot be byte-identical, by construction.** stage-0 drives LLVM's C
API and LLVM renders the IR; `emit.bx` writes IR as text. Two people giving directions to the same
place, one saying *"left at the church"* and one *"left after 200 metres"*. Forcing them to agree
would mean writing an LLVM-IR pretty-printer for the sole purpose of matching a string, which
improves nothing about the compiler. **The claim already asserted is stronger**: 143 of 143 pass
programs compiled by BOTH print the same bytes when run, 30 of 30 panic fixtures still fail, and
stage-1's own source reaches a byte-identical fixpoint. That is arriving at the same destination
rather than the directions rhyming.

**`typeck.rs` against `check.bx` is the same shape one level up.** The verdicts are already an
`assert_eq!` — 271 of 274, every fixture, an equality and not a floor. Only the wording differs, in
267 of the 271: two proofreaders catching the same typo and writing different notes in the margin.
**A different verdict is a defect; a different sentence is a preference.** Requiring identical text
would gate the row on rewriting 267 messages for no gain in correctness, and whether the text should
converge is §task 15, held deliberately apart.

**Andre's ruling settled it**, when the question was put rather than decided alone:

> *"The 2 out of 11 — if the output is the same, just wording and message different, for me that is a
> pass, and you can check them and put as done."*

and then generalised it, which mattered more than the ruling:

> *"When I say equal it doesn't mean identical literal. I said it basing on the output/result. Burxt is
> not Rust and vice versa, so there will always be difference. As long as we can give the same result
> in the Burxt way, that is a yes for me. Think outside the box — novelty and originality."*

**That is a better bar than mine, and the reason is not leniency.** Byte-for-byte output quietly
assumed the Burxt implementation should be a TRANSLATION of the Rust one. A transliteration would
inherit the original's bugs — and this one has instead **found three of them**, precisely because it did
each job its own way:

| The Burxt way | What it found |
|---|---|
| `diag.bx` counts bytes, so it is total | `diag.rs` sliced strings and **panicked** — `let é: Int = ;` gave a Rust backtrace and exit 101 instead of a diagnostic |
| `lsp.bx` resolves imports and appends the editor's buffer after them | `lsp.rs` answered hover **on no file with a `use` line** — every real Burxt program — and had been dead there for as long as hover existed |
| `lsp.bx` scans for the one key it wants instead of building a Value tree | *absent* rather than *parse error* as the failure mode, which is the right one for a server that must not die on a malformed message |

A bar demanding identical output would have called all three "not yet verified", and the third one
"Partial" — a design read as a shortfall.

So the map gained a fifth strength level, `Behaviour`, those two rows are **DONE** rather than pending,
and the bar is:

> **Every row is held by the strongest comparison its nature allows, and that comparison is NAMED.**

Which is a bar that can be met and cannot be met vacuously — the naming is what stops it becoming a
shrug. Where the answer is "byte-for-byte", nothing less will do. Where byte-for-byte is impossible,
the behavioural claim is written down and asserted, and the reason it is the ceiling is written beside
it.

| | Count |
|---|---|
| Rust modules answered | **11 of 11**, 0 Rust-only |
| Held **byte-for-byte** | **4** — `diag`, `schema`, `lsp`, `review` |
| Held by **behaviour** — same result, the Burxt way | **7** — `codegen`, `typeck`, `lexer`, `ast`, `parser`, `json`, `main` |
| **Unheld** | **0** |

## THE GATE IS MET — v0.0.239

Every one of the eleven Rust modules is held by the strongest comparison its nature allows, and the
ratchet is an **equality** rather than a floor: adding a `.rs` file without a comparison now fails the
suite, and so does deleting one.

The last row was `main`, and it closed with `--json` and the caret block. **Both `Partial` and `Role`
were deleted along the way, and in each case `-D warnings` is what told me** — an unused enum variant
means no row is left in that category, and a category nobody occupies reads to the next person as a
level someone ought to be climbing out of. Two levels remain and both are passing ones, which is the
honest shape now.

**One subcommand is absent and it is BLOCKED, not unwritten.** `explain memory` reads the allocation
inference, which is stage-0's alone: stage-1 requires the `allocates nothing` marker rather than
deriving it, which is why M14 slice 1 shipped its two halves two versions apart. It closes with **A12**
— and so do the three `allocates_nothing_*` fail-fixture exclusions, which would make the refusal
equality complete with no exclusions at all.

### What the gate cost, and what it bought

It bought a compiler that builds itself, runs programs, cross-compiles to arm64, serves LSP, derives
MCP manifests, reviews promises and prints layouts — and a suite where CI runs the **Burxt** runner
first and gates on it.

What it found is the better argument. **Six real defects, none of which a reading would have produced**,
and every one because the second implementation did the job its own way rather than by transliteration:

| | |
|---|---|
| `diag.rs` **panicked** — Rust backtrace, exit 101 — on a span ending mid-character | `diag.bx` counts bytes and is total |
| `lsp.rs` answered hover on **no file with a `use` line** — every real program — and had been dead there for as long as hover existed | `lsp.bx` resolves imports and appends the buffer after them |
| the `?` operator had **no implementation at all** on the Burxt side while the suite reported 143 of 143 | no fixture used it, and the sweep named seven examples BY HAND |
| a **silent use-after-free**: `region` storage assigned outward read back as the print's own buffer | the checker had MARKED the hazard and cleared the mark |
| `check.bx` **refused a valid program** — the worse direction, and the sweep never read the checker's verdict | one fix closed gaps in three tools |
| a diagnostic in an importing program reported **line 1543 of a 13-line file** | invisible until `check` could print a position at all |

The last one is the shape worth remembering: **a missing capability hid a missing invariant.** No code
on the Burxt side had ever needed to know which file an offset fell in, so nothing noticed that
`modules.bx` kept no source map — until the compiler learned to say where a problem was.

**So the gate is 10 of 11 held, and the last one is a missing FEATURE rather than a disagreement.**
`main.bx` has `check`, `check -`, `build`, `run`, `emit-ir`, `--target`, `layout`, `review`,
`mcp-schema`, `lsp`, `--version` and `--help`. It owes `--json` and `explain memory` — and the second
is honestly blocked, because the allocation inference is stage-0's alone until A12 lands.

The `Role` level was **deleted** in the same version, and the compiler is what told me to: `-D warnings`
refused an unused enum variant. Under the old bar `Role` was a waiting room for evidence that fell
short of byte-for-byte. Under *same result, named comparison* the category dissolves — the front-end
sweep compares two lexers' and two parsers' verdicts across all 160 sources and requires zero
disagreements, which IS a direct comparison of the result. Calling it "indirect" undersold it for as
long as the wrong bar was in force.

**And the ruling retires work rather than only settling a label.** §task 15's second half was "converge
267 diagnostic messages". That is now **not required**: a different sentence describing the same
refusal is a pass. What remains required is the thing already asserted — the same *verdict* on every
fixture, which is an equality with three named exclusions. Nobody has to rewrite 267 messages, and
`lsp.bx`'s decision not to build a translation table was right for the same reason.

The five remaining are the real work. `lexer`/`ast`/`parser` could be compared byte-for-byte via a
token or AST dump — but that dump would have to exist in **both** compilers in the same version,
because adding it to the Burxt side alone would make it more capable than the Rust one, which under
this gate is a defect and not a feature. That cost is worth pricing before committing to it. `json`
is arguably MIS-MAPPED rather than incomplete: the Burxt compiler does not use `lib/json.bx` at all —
`lsp.bx` hand-wrote its own scanner, deliberately, so that the compiler does not depend on the
standard library. And `main` owes `explain memory`, `--json` and the caret diagnostics.

**The five tasks, in order.** Created as tasks so they cannot be forgotten between sessions:

| # | Task | Why here |
|---|---|---|
| 1 | **`main.bx` grows the full CLI** — `check` (+`--json`, +`-`), `build`, `run`, `emit-ir`, `layout`, `explain memory`, `review`, `mcp-schema`, `lsp`, `-o`, `--target` | The centre of the gate: it is what makes the Burxt-built compiler a **drop-in** rather than a backend. `main.rs` is 572 lines with ten subcommands; `main.bx` is 118 with none |
| 2 | **`schema.bx`** — `mcp-schema` | The strongest single capability claim in the project. While it is Rust-only, the claim rests on Rust |
| 3 | **`review.bx`** — `review` | §C2 makes this the mechanical semver rule for the 1.0 promise. Rust-only means Burxt cannot enforce its own compatibility promise |
| 4 | **`lsp.bx`** — the language server | No longer blocked; see the correction above |
| 5 | **Raise `verified` to 11** | The gate is not met until the rows are compared, not merely populated |

### What the parity work FOUND — the argument for doing it at all

Three defects, none of which a reading would have produced:

1. **`diag.rs` panicked.** `let é: Int = ;` — a Rust backtrace and exit 101 instead of a diagnostic,
   because `lexer.rs` ended an unknown-character span one BYTE into a two-byte character and
   `diag.rs` sliced there. `diag.bx` counts bytes, so it is total where the Rust one was partial: it
   rendered the input correctly the whole time. **The differential test running in the direction
   nobody expects** — the second implementation auditing the first.
2. **The `?` operator had NO implementation on the Burxt side**, and the suite reported 143 of 143.
   `?` shipped in stage-0 long ago; `examples/absence.bx` was written to show it off; **no
   `tests/pass/` fixture used it**; and the front-end sweep cross-checked seven example paths NAMED
   BY HAND with `absence.bx` not among them. So the only user of the feature in the repository was
   never run through the Burxt front end, which refused the character outright.
   **A hand-maintained list of files is a directory boundary, and a new file lands on the wrong side
   of it in silence** — the third time this repository has paid for that shape. The sweep walks the
   directory now. Fixed in v0.0.221 with a pass fixture and **a fail fixture per refusal**, because
   a pass fixture cannot tell "supported" from "not examined", which is how the gap survived.
3. **The two compilers disagreed on a program with no errors.** Rust announces success with
   `eprintln!` and `main.bx` used `print`. Nobody looks for a parity bug in the success path. The
   rule is now deliberate and tested: **status to stderr, product to stdout.**

And one honest non-finding: `burxt build` was leaving `<name>.ll` and `<name>.o` beside the
executable where the Rust build leaves only the executable. Found because a comparison I ran from
the repository root tripped the root-cleanliness invariant with my own droppings.

**Two capabilities were verified present before any of this was scheduled**, because the alternative is
scheduling a language decision that is not needed:

- **`build` and `run` need to invoke `llc` and `cc`** — `external function system(command: String) -> CInt touches commands` already exists in `lib/os.bx`.
- **`check -` and `lsp` need stdin** — `external function getchar() -> CInt touches input` already exists there too, and was measured reading a framed LSP message.

### A0d — the 32 accept-side gaps, enumerated at last (v0.0.224)

**Where stage-0 refuses and stage-1 accepts, no test can see it.** The differential asserts
`caught >= 242` — a **floor** — so every gap hides underneath a passing suite. That is this
project's own rule at scale: *"where stage-0 refuses something, stage-1's handling is untested by
construction."*

It was 43 when first measured, and it is 32 now. **Nobody had ever listed them**, which is why the
count could drift: a number with no names attached cannot be worked on.

| Group | Fixtures | Verdict |
|---|---|---|
| **`allocates nothing`** | `allocates_nothing_broken_directly`, `..._through_a_call`, `..._through_a_dynamic` | **DELIBERATE.** The allocation fixpoint is stage-0's alone; stage-1 requires the marker rather than deriving it. M14 slice 1 shipped them two versions apart for this reason. Closes with A12 |
| **The FFI boundary** | `boundary_cdouble_return`, `boundary_decimal_needs_marshaller`, `boundary_marshal_on_burxt_function`, `boundary_marshal_on_non_decimal`, `boundary_unknown_marshaller`, `string_extern_return`, `c_bytes_at_refuses_a_negative_literal` | **GAP, 7.** The largest group, and the one where being wrong is worst: these rules are what stop a Decimal crossing into C as a float |
| **Interfaces and `dynamic`** | `trait_dyn_from_expr`, `trait_dyn_mut_method`, `trait_dyn_return`, `class_implements_wrong_return` | **GAP, 4.** All four are about an interface object borrowing the value behind it |
| **Arrays** | `array_return`, `array_zero_len`, `slice_nested`, `let_inference_needs_a_type` | **GAP, 4** |
| **`mutable` parameters** | `mutable_argument_must_be_mutable`, `mutable_argument_needs_a_home`, `mutable_method_parameter` | **GAP, 3.** Shipped v0.0.201 and stage-1 never learned to refuse the misuses |
| **String braces** | `brace_bare_close`, `interp_empty`, `interp_unterminated` | **GAP, 3.** Same family as the `\u` and raw-NUL gaps already closed |
| **Records: `==` and `<`** | `class_equality_needs_comparable_fields`, `class_has_no_ordering` | **GAP, 2** |
| **Decimal limits** | `decimal_literal_precision`, `decimal_scale_cap` | **GAP, 2.** Scale > 18 does not fit a scaled i64 — accepting it silently is a wrong ANSWER, not a missing diagnostic |
| **Odds and ends** | `bool_order`, `neg_of_string`, `region_name_clash`, `method_receiver_names_no_parameters` | **GAP, 4** |

**3 deliberate, 29 gaps.** And the ratchet stays a floor only until the 29 are closed; then it
becomes an **equality** over everything but the three, so another gap can never hide again. That
conversion is the point of the exercise — the list is a means, the equality is the end.

**The wording question, separately.** Of the 242 both compilers refuse, **4 word it identically**.
That is not "nearly aligned", it is unrelated text that happens to refuse the same programs. The
recommendation on record: require the same refusal SET (an equality) and the same LINE, and let
wording converge fixture by fixture as each is touched. Never a translation table — that would hide
the divergence rather than fix it, which is what `lsp.bx` deliberately refused to do for its
diagnostics.

## The ordering rule — SUBORDINATE to the gate above

> *"I would rather do compiler fixes to unblock a lot first, second to bugs that is urgent."*

So: **A** compiler leverage → **B** urgent bugs → **C** the rest of the bar → **D** the library floor →
**E** security → **F** papercuts → **G** post-1.0 → **H** the release gate.

**One correction to this section's own premise, earned in v0.0.250.** A5 was listed as a `M` compiler
fix and turned out to need **no compiler change at all** — its four functions are library code over
`byte_at` and `bit_and`, and both compilers accepted them unchanged on the first try. So §A is not
"compiler work"; it is "work that unblocks a lot", and at least one row was in the wrong section.
**Measure a row before scheduling it** has now retired three: A1 (`c_bytes_at`, already shipped), `==`
on records (already working), and A5 (not a compiler item at all).

**Why A before D, concretely.** If the ~120 library items are written before A2 (`const`), A3 (generic
`Option`) and A5 (`.chars()`), then `lib/math.bx`, `array_pop` and `string_reverse` get built *around*
those limits and rebuilt afterwards. A-first means each is written once. And two B items already depend
on A items — B5 needs A1, B9 needs A5 — so A-first is the correct dependency order, not merely the
cheaper one.

## Three process rules that govern all of it

Each was learned expensively and each has a version number behind it.

1. **A status line saying DONE is not evidence. The suite is.** M13 was marked DONE for fourteen
   versions with its core decision (`it`) never implemented and a cited fixture that had never existed.
2. **A pass fixture cannot tell "supported" from "not examined."** A new type going green in stage-1 on
   the first try is a red flag — check the fail fixtures and measure the ratchet. This caught `CPointer`
   (v0.0.196) and a shift-distance node kind (v0.0.199).
3. **Where stage-0 refuses something, stage-1's handling is untested by construction.** So relaxing any
   stage-0 rule is itself the reason to go read stage-1's version of it. That is how v0.0.202 found
   stage-1 answering `<` on Strings with `strcmp(a,b) == 0`, and generic comparisons sorting by
   allocation address.

---

## A0 — DONE (v0.0.214): the Burxt compiler stopped living in `examples/`

Andre, on reading the layout: *"these are not examples but a working compiler, priority number 1."*
Correct, and it had been wrong since the compiler was first split out. The self-hosted compiler is this
project's **capability certificate** — stage-1 compiles its own source and stage-2 emits byte-identical
IR for it — and it was filed under a directory whose name says "sample code."

`A7.0-NAMING.md` exists to stop exactly this, and its own rule is *"a name that reads as the wrong thing
is worse than a name that is merely short."* That rule had been applied to keywords and to functions, and
never to directories.

Both compilers now sit under `src/`, each named by the language it is written in:

| | Language | Lines | Role |
|---|---|---|---|
| `src/rust-compiler/` | Rust | 18,974 | the trust anchor and the **differential test** — no longer a bootstrap |
| `src/burxt-compiler/` | **Burxt** | 10,981 | the compiler written in itself, and the one that compiles itself |

### A0b — DONE (v0.0.215): and then the entry point was still called `stage1.bx`

Andre, on reading the fixed directory: *"the rust compiler is well named and categorized, while the burxt
is almost ok but has stage1 — what is stage1 if the maintainer is a human?"*

Right, and it is the same defect one level down. Every neighbour is named for **what it does** —
`parser.bx`, `check.bx`, `emit.bx`, `ast.bx`, `modules.bx` — and the entry point alone was named for
**its position in a bootstrap sequence.** "Stage 1" answers *which step of building something else is
this*, a question only whoever built it ever asks. A maintainer asks *where does the program start*, and
that is spelled `main` in every language there is — including in `src/rust-compiler/main.rs`, sitting in
the other compiler at the same place doing the same job.

It had also stopped being **true**. A stage is a step toward a destination not yet reached; this compiles
itself to a byte-identical fixpoint, so it is not on the way to a compiler, it **is** one. The filename
outlived what it described — the identical rot to §3b above, in the same version, which is why both are
recorded rather than quietly fixed.

So `src/burxt-compiler/stage1.bx` → **`main.bx`**, and the two directories now read as the same program
twice:

| `src/burxt-compiler/` | | `src/rust-compiler/` |
|---|---|---|
| `main.bx` | the entry point | `main.rs` (+ the CLI) |
| `ast.bx` | shapes, lexer | `ast.rs` + `lexer.rs` |
| `parser.bx` | tokens in, arena AST out | `parser.rs` |
| `check.bx` | scales, regions, purity, contracts, exhaustiveness | `typeck.rs` |
| `emit.bx` | textual LLVM IR + the runtime | `codegen.rs` (LLVM's C API via inkwell) |
| — | **the tooling, and nothing on the Burxt side yet** | `lsp.rs` · `review.rs` · `schema.rs` · `json.rs` · `diag.rs` |

That last row is the whole asymmetry, and B12 is now pointed at it.

**"stage-0" and "stage-1" survive as PROSE**, in the specs, where the subject really is the bootstrap:
which compiler built which, and why two of them must agree. That is a defined term doing work in a
sentence about history. A filename is not a sentence.

**The rename cost three broken tests and produced a ninth naming failure mode** — `burxt build` with no
`-o` *derives* the binary's name from the filename, so `scratch.join("stage1")` was a reference no sweep
could see. `spec/A7.0-NAMING.md` §9 has it; the fix was to stop deriving rather than to sweep harder.

**Why the Rust one is larger — and the first answer here was WRONG, which is worth keeping.**

v0.0.214 wrote: *"stage-1 is a SUBSET. Its backend does not emit Decimals and their rounding, `match`,
`musttail`, contracts, or the FFI boundary."* That came from `M4-SELF-HOSTING.md` §3b, was true when §3b
was written, and **is now false.** Nothing updated §3b as each feature landed, and this document
believed it and re-published it a version later.

**Measured as of v0.0.215:** stage-1 compiles **143 of 143** pass programs, **0 refused** — including
`match`, `Decimal` with rounding contracts, `requires`/`ensures`, `external function`, `decreases`,
`return tail`, the generic-heavy `lib/array.bx`, the exact-vector library, `lib/test.bx` and the pointer
wall. Their binaries run and match stage-0's output. It also compiles **itself**, 2.6 MB of IR, to a
byte-identical fixpoint.

> This project's rule is *"a status line saying DONE is not evidence. The suite is."* The correction is
> that **a status line saying NOT DONE is not evidence either** — and this one had a test printing
> `143 of 143` beside it the whole time.

So the ~8,000-line gap is **not capability**. It is:

| | |
|---|---|
| ~2,632 lines | tooling stage-1 does not have — LSP, `burxt review`, `mcp-schema`, the JSON emitter, diagnostics rendering, the CLI |
| ~5,400 lines | stage-0 drives **LLVM's C API** through inkwell; stage-1 **writes textual IR**, which M4 calls *"not a workaround — string formatting instead of an API"* |

Neither is Burxt failing to express something.

**What this changes about stage-0's role.** It is no longer a bootstrap — Burxt builds Burxt. Its
remaining job is a **test oracle**, and that job earns its keep empirically: five silent wrong answers in
one week were found because two implementations disagreed. The cost is equally real — every feature needs
stage-1 parity, roughly doubling the work — and `M4` §5's *"retired or kept as reference"* was always a
choice. Keeping it as an oracle while dropping the bootstrap framing is the honest position.

**And it re-points B12.** The gap worth closing is not the language, it is the TOOLCHAIN: stage-1 has no
LSP, no `review`, no `mcp-schema`. That is where all the interesting non-compiler work still lives in
Rust, and moving it is a real capability claim — unlike the subset claim, which was false.

### A0c — IN PROGRESS (v0.0.216): every `.rs` gets a `.bx`, and it is a ratchet

Andre: *"make sure all rs compiler has a burxt equivalent — that is the true meaning of both compilers
agree."*

A sharper bar than the one being met, and the sharpening is the point. "Both compilers agree" had come to
mean **the language** is covered twice — 143 of 143, a byte-identical fixpoint, 30 of 30 runtime
guarantees, all true. But the agreement stopped where the compiler proper stops, so every claim Burxt made
about **tooling** was a claim about what Rust can do with a Burxt AST.

`every_rust_module_has_a_burxt_counterpart_or_a_reason` now holds the whole directory to account: each
`.rs` is mapped to its counterpart or listed as missing **with a reason**, the mapped count is a ratchet,
and a new `.rs` with no row fails the suite — so the decision is forced when the file is written rather
than in an audit a hundred versions later. That timing is the entire lesson of §3b.

| Rust module | Counterpart | Strength | Note |
|---|---|---|---|
| `diag.rs` | `diag.bx` | **VERIFIED** | held byte-for-byte by `the_two_compilers_render_a_problem_identically` |
| `parser.rs` | `parser.bx` | Role | held *indirectly*: the front-end sweep and the 142-of-142 backend sweep both fail if the parsers disagree |
| `typeck.rs` | `check.bx` | Role | held indirectly over 273 fail programs, 227 of which stage-1 refuses too |
| `codegen.rs` | `emit.bx` | Role | held indirectly, and byte-for-byte, by the fixpoint |
| `lexer.rs` | `ast.bx` | Role | shares one file with `ast.rs` |
| `ast.rs` | `ast.bx` | Role | the same file answers both rows |
| `main.rs` | `main.bx` + `modules.bx` | **Partial** | 572 lines with ten subcommands, against a `main.bx` with none |
| `json.rs` | `lib/json.bx` | **Partial** | the standard library, which the Burxt compiler does not itself use |
| `schema.rs` | — | Missing | writable today |
| `review.rs` | — | Missing | writable today, and C2 depends on it |
| `lsp.rs` | — | **Blocked** | see below |

### The count, since Andre asked *"7 over 11?"*

The scrutiny was deserved and the first number was flattering. **8 of 11 rows are answered — but only
1 is VERIFIED**, and the difference is the whole point:

- **`lexer.rs` and `ast.rs` point at the same `ast.bx`**, so one Burxt file earned two points. Eight
  rows are answered by eight distinct files only because `main.rs` answers to two.
- **`main.rs` → `main.bx` is generous.** 572 lines carrying ten subcommands against 118 with none. Same
  entry point, not the same job — so it is recorded `Partial`, not `Role`.
- **`json.rs` → `lib/json.bx` is generous.** That is the standard library; the Burxt compiler does not
  use it, and `diag.bx` hand-writes its own escaping exactly as `diag.rs` does. The capability exists
  in Burxt; the compiler-internal counterpart does not.

So the test now counts three ways and ratchets on two — `answered >= 8` **and `verified >= 1`** — and
`verified` is the one to raise, because a direct comparison is the only thing that turns *"a file with
that job exists"* into *"the two agree."* Four `Role` rows are held hard but indirectly: the fixpoint
and the two sweeps fail if those pairs diverge on anything in the suite, which is real evidence and is
not a comparison.

**It also found a hole in its own map.** Keying on `.rs` files meant a Burxt file no row mentions was
invisible — and `modules.bx` was exactly that, because its Rust counterpart is `load_program` *inside*
`main.rs`. It could have been deleted without failing anything. The test now walks both directions.

Two rows are ordinary work. The third is a real finding:

> ~~**`lsp.bx` is unwritable, not unwritten.**~~ **WRONG, corrected in v0.0.218 by running it.** This
> said a language server frames messages over stdin, Burxt cannot read stdin, and `fread` is out of
> reach because a caller cannot produce a pointer to writable memory — so the row needed *"a stdin
> primitive designed first, and that is a language decision, not a port."*
>
> **`external function getchar() -> CInt touches input` was already declared in `lib/os.bx`, and
> already in use.** A Burxt program reads a framed LSP message off stdin today; measured, 39 bytes of
> `Content-Length: 42\r\n\r\n{...}`. No new primitive, no language decision, no wall.
>
> This is the wall pattern's ninth sighting and the worst-timed one: **I reasoned about the wall
> instead of walking up to it, two versions after adding `no_document_claims_a_coverage_number_the_suite_refutes`
> for exactly this failure.** The rule that catches numbers does not catch prose — *"there is no way to
> do X"* has no number in it. So the habit is the only instrument: **before writing "blocked", run the
> smallest program that would prove it.** Five lines and one minute, against a row that would have sat
> in a roadmap as a language-design question.

**And writing the first counterpart found a crash in the original**, which is the argument for the whole
exercise stated better than I could state it in advance. `let é: Int = ;` made stage-0 **panic** — a Rust
backtrace and exit 101 — because `lexer.rs` ended an unknown-character span one BYTE into a two-byte
character and `diag.rs` then sliced the source at that non-boundary. `diag.bx` rendered it correctly the
whole time: it counts bytes, so it is total where the Rust one was partial. **The differential test
running in the direction nobody expects** — the second implementation auditing the first rather than
being checked against it. Fixture: `tests/fail/non_ascii_identifier.bx`.

The one interesting problem in `diag.bx` **dissolved**, which is the wall pattern's eighth sighting:
`diag.rs` counts columns in CHARACTERS and Burxt has no `.chars()` (that is A5). But the question was
never *"can I iterate codepoints"* — it was *"can I count them"*, and in UTF-8 the count is just the
bytes that are not continuation bytes (`bit_and(b, 192) != 128`). Exact, no approximation, no waiting for
A5, nothing named as a limit.

## A — Compiler fixes, ranked by leverage ÷ cost

| # | Fix | Size | Unblocks |
|---|---|---|---|
| ~~A1~~ | ~~**`c_bytes_at(p, n)`**~~ **DONE, and the ☐ was stale** — shipped with `tests/pass/c_bytes_at.bx`, three fail fixtures and two panic fixtures. Found by MEASURING before scheduling it, which is the rule this roadmap keeps re-learning: a status line saying NOT DONE is not evidence either | — | **Every key, token, session and UUID v4** — `/dev/urandom` is a character device, so `read_file` sizes it with `ftell` and gets 0 · `file_read_bytes` · streaming reads · `mmap` → N9 row 6 · buffer-filling syscalls (`getrandom`, `clock_gettime`) |
| ~~A2~~ | ~~**`const` / named constants**~~ **DONE v0.0.243**, both compilers, 18 fail fixtures, all refused by both. **The section's premise was right and understated:** the blocker was not awkwardness, it was that **a module cannot hold a statement** — `main.rs` refuses a top-level `let` in any file reached by `use` (*"a module holds declarations, not statements"*, M6 §1.3), so `lib/math.bx` could not have named `INT_MAX` under any arrangement. And **`INT_MIN` cannot be written as a literal at all** — `-9223372036854775808` lexes as a negation of an out-of-range literal — so constant FOLDING is load-bearing rather than a nicety: the one value A2 exists to name requires it. Measured both | — |
| ~~A3~~ | **DONE — the ⚠ was stale, and it was stale in the direction nobody re-tests.** Verified by running it: a free generic `function first_of<T>(xs: [T]) -> Option<T>` returning `Option.None` compiles and prints. `array_pop<T>`, a generic `Set`, `map.take` and `option_ok_or` were never blocked. ~~**`Option.None` in a free generic function.** ⚠ **Verify first.** `map.find` already returns `Option<V>` from a METHOD, so the limit is narrower than three library headers claim | **S–M**~~ | `array_pop<T>` · a generic `Set` · `map.take` · `option_ok_or` · retires a limitation cited in `array.bx`, `option.bx` and the audit |
| ~~A4~~ | ~~**`pure` on a method / `pure` returning an Option**~~ **DONE v0.0.248, and the two halves were ONE branch.** A variant constructor `Enum.Variant(x)` PARSES as a method call and is told apart inside the method-call branch — but the blanket *"a pure function may not call a method"* refusal sat at the TOP of that branch, before anything checked whether the receiver was an enum. **So a constructor was refused for being SHAPED like a method call**, and one removal fixed both items. `lib/array.bx`'s comment had named that mechanism exactly, years before anyone acted on it. **The row also understated the payoff:** `typeck.rs` has always checked a method's clauses with `in_pure` set, so `requires self.sum() > 0` was already asking whether `sum` is pure and being refused by the blanket branch — the whole second half of the item is that the answer can now be yes | — |
| ~~A5~~ | **DONE — shipped in `lib/string.bx` and it needed NO compiler change**, which is why the row outlived the work. `next_char`, `char_count`, `char_at`, `codepoint_at`, `from_codepoint`, `is_valid_utf8` and the rest, 24 codepoint entry points. Mis-filed under §A, which is for compiler items. ~~**`.chars()` / codepoint iteration** — A4.4's one remaining gap | M~~ | The whole UTF-8 layer: correct case handling · a `string_reverse` that does not corrupt · char indexing · `\uXXXX` in JSON · `is_valid_utf8` |
| ~~A6~~ | ~~**`for i in 0..n`**~~ **DONE v0.0.245**, both compilers, ten fail fixtures, all refused by both. **Exclusive only, and no inclusive form** — three reasons: `0..len(xs)` is the same bound `while i < len(xs)` already writes, half-open ranges tile with no gap (which is why `substring(s, from, LENGTH)` is half-open too), and two forms one character apart where that character changes the iteration count is precisely what a reviewer's eye slides over. Cost named: `0..n + 1` shows a visible `+ 1` rather than an invisible `=`. **`for`-only, not a value** — a range as a value wants an iterator protocol (A11) and half of one is worse than waiting. **Reversed LITERALS refused, reversed computed bounds run zero times**, because `for i in 0..len(xs)` over an empty array is correct code that must not trap | — |
| ~~A7~~ | ~~**Integer widths** `i32`/`u8`/`u32`/`u64`~~ **DONE v0.0.261, both stages.** Boundary-only, one `Type::Width { bits, signed }`, per-width trap at exit 70. All five boundary refusals byte-identical across the compilers; `strcmp -> i32` comes back **negative** and `strlen -> u8` on 200 chars = **200** — the two cases where sign- and zero-extension disagree. **Corrected v0.0.264: this row said `= -1`, and -1 is glibc/x86-64's answer, not C's.** C specifies only the SIGN of `strcmp`; glibc/aarch64 returns -64. I published a libc's implementation detail as a measured fact because every machine I measured on was x86-64 — the same root cause as B18, found by the same arm64 runner. The non-mechanical part was that **stage-1 has no `validate_type` choke point** — the rule lives in `parse_type` behind an `in_extern_signature` flag, and once it existed, giving `CInt` the same rule (§B16) cost three lines. The choke point was the whole cost; which type attaches first is incidental | — | C structs (`dirent.d_name`) · fixed-width records → N9 row 6 · `clock_gettime` → **monotonic and sub-second time**, so benchmarking and timeouts · binary formats · A4.4's deferred **Bytes type** |
| ~~A8~~ | **DONE v0.0.276, both compilers.** `(Int, String)` as a type, `(1, "a")` as a literal, `pair.0` positional access, and a function returning two values — which is the point, and what `zip`, `enumerate`, `char_indices`, `split_at`, `divmod` and `split_once` were all waiting on. **Positional access rather than destructuring**, chosen for being fewer moving parts: `match` already binds patterns, so destructuring can be added later without invalidating anything pinned now. **It met the escape rules rather than going silent behind them** — a tuple holding a String is region storage and is refused on the way out, with the same sentence every other route gets; reading its Int member out is a copy and is accepted. That check was run deliberately, because a new type going green first try is this project's most reliable red flag (A7 shipped exactly that way once). Corpus 126/126, 0 divergences, fixpoint intact. ~~**Tuples** | M~~ | `zip` · `enumerate` · `char_indices` · `split_at` · `divmod` · `split_once` without inventing a record |
| ~~A9~~ | **DONE v0.0.277, both compilers — and it RETIRES A10.** `interface Mapper<T>`, `implement Mapper<Int> for Doubler`, and `dynamic Mapper<Int>` passable and callable as a parameter. **The acceptance demonstration is a real `map` over an array**, which is what A9 was held to and why closures are now declined rather than deferred: `function map_ints(xs: [Int], f: dynamic Mapper<Int>) -> [Int]` prints `2 4 6` in both compilers. `map`, `filter`, `fold`, `sort_by`, `any`, `all`, `retain`, `partition` and `position` are now writable across four libraries. **It shipped a live use-after-free first, and the corpus said nothing.** `dyn_call_relays` keyed on `Holder` where the type is `Holder$String`, so a generic interface relaying region storage was accepted by stage-0 and refused by stage-1 — `kept = h.get()` printed `secret-value` then the clobber. **133 corpus programs reported 0 divergences on it**, because none of them relays through a type that did not exist when they were written. Found by probing by hand, which is now the standing rule: **when a type is added, probe the escape rules against it before believing anything green.** Sixth time a new type went silent behind a rule. ~~**Generic interfaces** — the cheap alternative to closures. `dynamic Trait` is already a function value in all but name; interfaces simply cannot take type parameters. **On no roadmap — needs an explicit yes/no, because YES may replace A10** | M~~ | `sort_by` · predicates · visitors · most of `map`/`filter`, in a form consistent with the no-closures decision |
| ~~A10~~ | **DECIDED: NOT BUILDING IT. A9 replaces it.** The row asked for an explicit yes/no and carried it open for months; here it is, decided on measurement rather than argument. **`dynamic Trait` is already a function value** — this runs today and prints 16: `function twice(s: dynamic Step, x: Int) -> Int { return s.apply(s.apply(x)); }`. Behaviour is passable as a value, callable through, and captures state in the implementor's fields. **Exactly one thing is missing**, and it is A9: an interface cannot take a type parameter (`interface Mapper<T>` gives `expected '{', found '<'`). That single gap is why `map`/`filter`/`fold`/`sort_by` are absent from four libraries at once — each needs `Predicate<T>` or `Mapper<T, U>`, and an interface can currently only be written for one concrete type. **So closures would be a second way to do what one already works for**, at L instead of M, in a language whose case is that a reviewer can see what the code does. The historic blocker — *"a closure needs an owner for its captured state, a memory question"* — was answered by A12, so this is a choice rather than a deferral. **Reopen it only if A9 fails to produce a clean `map`**, which is the acceptance demonstration A9 is being held to. ~~**Closures / function values** — or A9 instead of it | **L**~~ | `map`/`filter`/`fold`/`any`/`all`/`retain`/`partition`/`position` across four libraries **at once** · `signal()`, so a server can shut down cleanly |
| ~~A11~~ | **DONE v0.0.278, both compilers. §A IS COMPLETE.** And it was not the L the row estimated — measurement made it one capability. A9 already made the whole protocol *expressible*: `interface Iterator<T> { function next(mutable self) -> Option<T> }` parses, implements and coerces. **Exactly one thing blocked it**, and the compiler said so itself: *"calling a mutating method through an interface object is not available yet — the compiler still cannot tell whether the value behind the object was declared mutable. Regions bound its LIFETIME, not its mutability."* So A11 was **mutability tracking through a `dynamic`** — the item re-diagnosed in v0.0.26 as not a memory problem, which is why A12 did not help. Acceptance: a real `Counter` driven to exhaustion through `dynamic Iterator<Int>`, printing **3** in both compilers; the immutable-source form **stays refused** in both; and an iterator over Strings escaping a region **is refused**, which was the probe rather than the suite — the suite, the fixpoint and the corpus were all green through A9's use-after-free. ~~**An iterator protocol.** A4.4 deferred it with *"trigger: after growable collections make iteration general."* **That trigger has fired.** | L~~ | Lazy chains · `for` over a Map without allocating an array and re-hashing every key |
| **A12** | **DONE — stage-0 v0.0.272, stage-1 v0.0.275.** All-or-nothing per block: a block releases only when the analysis proves nothing allocated inside it escapes, so a block that cannot prove it keeps its memory — today's behaviour — and the failure direction is memory, never a dangling pointer. **5,280 → 1,408 KB** on the non-escaping loop in stage-0, **7,744 → 1,408** in stage-1, the escaping variant unbounded in both **on purpose**. No allocator change; §10 intact. What made it buildable was not new analysis — the proof obligation *is* the escape analysis, which had **thirteen holes and eleven live use-after-frees** when this was last scoped (B20–B45). ~~**M14 slice 3 — per-block release** (+ ~~`allocates nothing`~~ v0.0.209, ~~`burxt explain memory`~~ v0.0.213 — **both cheap thirds done; the escape analysis remains**). ⚠ **IN PROGRESS — the forcing function fired at v0.0.207** | L~~ | Bounded memory in a loop (**5,280 → 1,408 KB** per 100k Strings) · the compiler's own ceiling, which **went red in CI at 544 MB against 540** while passing locally at 537 · **prerequisite for the freestanding/IoT target** |

### A1 in detail — why the smallest item is first

It was filed in `FAR-HORIZON-ROADMAP.md` M2 as one of four unopened doors, listed as an `mmap`
prerequisite. It is much more than that: **without it a Burxt process cannot generate a cryptographic
key at all.** `/dev/urandom` is a character device, `read_file` measures with `ftell` and gets zero, and
the only workaround — `os_capture("head -c32 /dev/urandom …")` — puts the key in the process table where
any other user on the machine can read it.

- Signature `c_bytes_at(p: CPointer, n: Int) -> [Int]`, copying `n` bytes into a region-allocated array.
- **The one real decision: what happens when `n` lies.** The length is the caller's claim, not a fact
  the type carries. **Resolved: trust it, name it in the documentation as the pointer wall's one soft
  edge, and check the half that can be checked** — a null pointer and a negative count, refused as a
  literal and trapped at runtime. Consistent with `as scaled` and `external function`: the boundary is
  declared, not inferred.
- Same shape as `c_string_at`, which already does the copy-at-the-boundary work. One extra argument.

> **✅ DONE (v0.0.207) — and one claim above needed correcting.** This section said `c_bytes_at`
> unblocks the CSPRNG. It does not do it alone. The chain is **`malloc` → `getrandom` → `c_bytes_at` →
> `free`**, and it works only because `malloc` returning a `CPointer` was already legal (v0.0.196) and a
> `CPointer` was already an allowed extern *parameter*. `c_bytes_at` is the last missing link, not the
> only one — so A1 completed something three versions in the making rather than opening it single
> handed. `tests/pass/os_random_bytes.bx` is the proof: 32 bytes of real OS entropy, in-process, in
> both compilers.
>
> Also landed with it, because they were the same hazard: **ten builtins that were implemented and
> never reserved** (roadmap B6) — `bit_*`, `shift_*`, `c_is_null`, `c_string_at`, `c_bytes_at` — so a
> program could declare a function with the same name and collide. And **eleven that were never
> documented** (B14), which is the same omission showing up twice, because
> `docs/reference/builtins.md` claims to be generated from that list. Each new entry carries a probe
> the generator **compiles**, so the page is verified rather than remembered.

---

### A7d — the integer-widths design, settled before it was built (v0.0.252)

An agent built representation, lexing and parsing end to end — `external function abs(n: i32) -> i32;`
lexed, parsed and reached the extern boundary check, clean under `-D warnings` — and then **reverted all
five files** rather than hand over a 30%-applied twelve-file change with codegen in it. Its own reason
is the best statement of why: *"that is exactly how today's two 'rules that compiled and enforced
nothing' happened."*

**The size estimate was wrong, and the reason generalises to this whole document.** Adding four type
keywords broke `editor_grammar_knows_every_keyword_the_compiler_does`; fixing that broke
`the_packaged_extension_matches_the_grammar_in_the_repository` and `the_reference_is_not_stale`. **Three
files nobody named** — because `M10` §2e is a rule (*a change to the language is not finished until the
highlighter, the language server and the packaged extension have changed with it*) and no estimate here
accounts for it. **Any row that adds a KEYWORD is three files larger than it looks** — and §A13 sharpened this: it is not keywords, it is anything landing in `is_reserved_name`, because that function's body is what the grammar test scrapes. A BUILTIN costs the same three files.

**The representation is the part worth keeping.** ONE variant — `Type::Width { bits: u32, signed: bool }`
— not four. `u8` versus `u32` is two numbers, and no `match` arm cares about the spelling: `llvm_type`
wants the bit count, the range check wants the bounds. `Decimal { scale, rounding }` is the precedent.
With `-D warnings` the compiler then NAMED every site needing an arm: **three, all in `codegen.rs`**
(`payload_cells`, `llvm_type`, `gen_print_value`). Four variants would have been four arms in each.

**Boundary-only keeps it inside the compiler, and that is measured.** `CInt` is already refused by name
in a `let`, a parameter and a class field — *"CInt only exists at the C boundary (external function
signatures) — use Int in Burxt code; values convert at the call."* So a width can never reach the layout
walk, which is why `layout.bx`, `layout_of`, `review` and the LSP need no arm at all.

**Trap at RUNTIME per width — settled by a fixture rather than by taste.**
`tests/panic/cint_range.stderr` PINS CInt's runtime trap, so adding a compile-time refusal for an
out-of-range literal would change CInt's existing contract. A literal check would be strictly better and
is a **separate decision**, not something to smuggle in beside this one.

**`u64` is a real limit and must be NAMED, not pretended.** `Int` is a signed i64, so a `u64` above
`INT_MAX` has no Int to land in. The honest shape: the range check's upper bound for `u64` is the SIGNED
maximum, and the message says so rather than claiming a range the language cannot hold.

**`review` needs no change, and why it differs from A4 is the reusable part.** A parameter going `CInt`
→ `u8` changes `Promise.shape`, which `review` renders from the Type's `Display`, so a width is carried
already. A4 needed work because `pure` is a **marker** the Promise struct did not hold. **Shape versus
marker.** Read from the path rather than run, since the work was reverted — verify when it lands.

**What remains:** two `typeck.rs` sites plus the boundary-only refusal, the per-width checked helper in
`codegen.rs` (generalise `i32 @burxt.checked.cint(i64)` to `burxt.checked.<bits>.<signed>`), all of
stage-1, and fixtures.

### ~~A13~~ — `byte_as_string(n)`: **DONE v0.0.260**, one builtin behind a whole cluster

**NEW, and it is the highest leverage-per-line item on this roadmap.** One builtin —
`byte_string(n) -> String`, a one-byte String for `0 <= n <= 255` — unblocks all of:

- `from_codepoint`, `from_bytes`, `from_byte` (§D1p)
- **`\uXXXX` decoding in JSON (§B9)**, so Burxt can read real-world JSON
- retiring `os_byte_as_string`'s lossy `"?"` for every byte ≥ 127 (**§B2**, a silent data-destroying bug)

**Why it is needed was PROVED rather than assumed**, in `lib/string.bx`'s own header, in four measured
steps: no builtin converts Int to a String byte (the full list is enumerated there); no escape names a
byte (`\xNN` and `\uXXXX` do not exist — checked against the lexer's refusal); so `substring` of a
LITERAL is the only Int-to-String path; and **a source file must be valid UTF-8**, so a byte ≥ 0x80 can
only reach a literal inside a complete multi-byte character.

That last step closes the door. Continuation bytes are cheap — `"ÀÁÂ…ÿ"` yields every byte
`0x80`..`0xBF`. **The LEAD bytes are the problem**: `0xC2`..`0xF4` needs 51 characters from 51 different
blocks, of which **six are unassigned codepoints** (`0xCC` combining marks, `0xEE` Private Use Area,
`0xF1`..`0xF4` four empty planes). Counted, not estimated.

**And the workaround was correctly REFUSED.** A 51-character table of mixed scripts and invisible
codepoints in the standard library is a liability, not a clever trick: a reviewer cannot read it,
`git diff` shows mojibake, and an editor that normalises Unicode silently breaks a whole plane. So it
is not written, and the decision is recorded where the functions would have been rather than made
silently by omission.

**The design is SETTLED as of v0.0.254 — four questions answered by measurement, and one of them
overturned this section's own premise.**

**The name is `byte_as_string(n)`, not `byte_string(n)`.** This language names behaviour AND direction —
`divide_floor`, `shift_right_zeros`, `string_to_upper_ascii`. `byte_string` names neither, and
`byte_string("a")` reads just as plausibly as the reverse conversion. `byte_as_string` states the
direction, mirrors the `os_byte_as_string` it retires, and does not borrow `to_string`'s meaning —
`to_string(233)` is three digit characters, a different conversion. Best property: it is the exact
inverse of `byte_at`, so **`byte_at(byte_as_string(n), 0) == n` for every 0..255** — one identity,
fixturable as a loop over all 256 values, which is a stronger pass fixture than any hand-picked set.

**Out of range: a compile-time refusal for a LITERAL, a runtime trap otherwise — CHOSEN, not
inherited.** CInt traps at runtime only and `tests/panic/cint_range.stderr` pins that, so CInt is
untouched; for a NEW builtin the literal check is free and strictly better, and the lexer already
refuses a knowably-bad literal (the sixteen-hex-digit rule). Two fixtures: `tests/fail` for the
literal, `tests/panic` for the computed value.

**It CAN manufacture invalid UTF-8, and its own doc comment must say so.** The capability already
exists — `read_file` reads arbitrary bytes, `c_string_at` copies whatever C hands over — so this adds no
new hole in §B5's declared-and-unenforced invariant. What changes is that the hole becomes reachable
from **pure Burxt** rather than only across the boundary: a change in kind, not degree. So the builtin
documents that it is the one builtin able to build a String that `is_valid_utf8` rejects, that it exists
to assemble a valid sequence byte by byte, and that `from_codepoint` is the safe layer above it.

**And the NUL hazard does not exist — which this section assumed it did.** A Burxt String is
**LENGTH-PREFIXED**: an i64 header at `s - 8`, so `len` is an O(1) header read. Measured in both
compilers: `len("a\0b")` is 3, `byte_at(s, 1)` is 0, `substring(s, 1, 1)` has length 1. So a NUL is an
ordinary byte and the full 0..255 range needs no carve-out. `tests/fail/string_raw_nul.bx` refuses a raw
NUL in SOURCE, which is source hygiene through a different door.

That was found because `emit.bx` still claimed *"`len` is therefore strlen, which is also why a String
has no length field"* — **load-bearing prose, stale, fourteen lines above the code that adds the
header.** Retired in v0.0.254. A stale sentence that gives a REASON is worse than a stale number,
because a reader can act on it, and an agent nearly did.

**A13 is a 15-FILE item, and the reason generalises §A7d.** `editor_grammar_knows_every_keyword_the_compiler_does`
scrapes the body of `fn is_reserved_name`, and §B6 requires reserving a builtin's name. So it is not
*keywords* that cost three extra files (grammar → `.vsix` → generated reference) — it is **anything that
lands in `is_reserved_name`.** Correct §A7d's wording accordingly.

**Everything else is ordinary Burxt on top of it.**

### A7e — v0.0.260 shipped A7's tooling ahead of its compiler, and why the measurement lied

**What is true:** at v0.0.260 the grammar, the `.vsix` and the generated reference list `i32`, `u8`,
`u32` and `u64`. **The compiler in that same commit does not know any of them** — `git show
HEAD:src/rust-compiler/lexer.rs | grep -c TyWidth` answers `0`, and a freshly built binary says
*"unknown type `u8` — declare it with `class u8 { ... }`"*. So the highlighter colours four types the
language does not have. That is a false promise to a reader, shipped by me, and this section exists
because the way it happened is worth more than the defect.

**How it happened.** An agent's A7 build was in the working tree. I measured it by running
`./target/release/burxt`, got a boundary-only refusal and a per-width runtime trap, and wrote both
transcripts down as evidence. Then the agent stashed its work — as I had asked it to, an hour earlier
and had forgotten — and I committed. **The binary was still the old build.** `cargo build --release`
said `Finished in 0.02s` because the source it was asked about had gone backwards, not forwards.

So the rule this repository already had — *DONE is not evidence, the suite is* — has a sibling:

> **A binary is not evidence of the source it came from.** `target/release/burxt` is an artifact of
> whatever the tree held when it was last built, and in a tree with another writer in it that can be
> minutes ago and three reverts away. Measure the source, or rebuild and measure, and when the two
> disagree believe `git show`.

And the sharper form, because the same greps ten minutes apart gave opposite answers:

> **You cannot measure a tree someone else is writing to.** Ownership by file was supposed to prevent
> this and did not, because the *artifact* is shared even when no file is. What is stable under a
> concurrent writer is `git show <commit>:<path>` and nothing else.

**Why nothing caught it.** `editor_grammar_knows_every_keyword_the_compiler_does` and
`the_web_highlighter_knows_every_keyword_the_compiler_does` both run **compiler → editor** and there is
no test in the other direction, so a word the editor knows and the compiler does not is invisible to
the suite. That is the same shape as the first process rule: a check that has never run looks exactly
like one that passes. **The missing invariant is the reverse direction** — every type the Burxt grammar
and the Burxt word lists highlight must be a type the compiler knows — and it lands with A7's compiler
half, when it can be green for the right reason rather than by having nothing to say.

**A second false green found while looking.** `the_web_highlighter_knows_every_keyword_the_compiler_does`
scraped all fourteen `words(...)` calls in `docs/assets/burxt-editor.js`, seven of which belong to the
PHP, Python and Rust snippets on the comparison page. So the test answered *"does this page know the
word at all"* under a name promising *"does the BURXT highlighter know it."* `i32`/`u8`/`u32`/`u64`
appear in that file exactly once — in **Rust's** type list — so the test was green while Burxt's own
list had none of them. Now scoped to the lists above `var PORTS`.

### A7f — how it actually closed, v0.0.261, and the two things that fell out

**A7 is done on both stages** and the reverse-direction invariant this section asked for now exists:
`every_type_the_editors_highlight_is_one_the_compiler_knows`, scoped to **types** because a blanket
reverse check cannot work — the grammar deliberately colours `fn`, `mut`, `impl`, `struct` and `trait`
as errors and those must stay unknown to the compiler. Mutation-tested rather than trusted: `u16` added
to the grammar and `f64` to the site list fails it, naming both. **It would have caught v0.0.260 on the
day.**

**The spelling-match trap caught me a second time, from the other side.** I told the agent not to re-add
the four widths to `docs/assets/burxt-editor.js` because v0.0.260 had put them there. It had not. Line
188 is the **Rust** word list the comparison page uses; Burxt's own `TYPE` list at line 46 had none of
them. I matched on spelling without checking which language's list I was in — the identical mistake the
scoping fix exists to prevent, made by the person who wrote the scoping fix, one hour later. The suite
settled it, which is the point of having one. **The VS Code grammar genuinely had been paid; the site
had not.** Two artifacts, one cascade, and "already done" was true of one and false of the other.

**A7 broke the fixpoint and A7 was not the cause — the ninth instance of the wall pattern.** Stage-2
died with `region memory exhausted`. **Stage-0 raised its arena to 4 GB in v0.0.222 and stage-1 never
did**, while `emit.bx`'s comment claimed *"the same reservation stage-0 makes"* — false for
thirty-eight versions. HEAD's stage-2 peaked at **1,042,976 KB against a 1,048,576 KB ceiling, a margin
of 0.53%**, and A7's ~80 lines took it. Any change of that size would have. Stage-1 built by stage-0
(4 GB) compiled `main.bx` at 1.016 GB and passed; stage-1 built by itself (1 GB) died. **The number was
the wall, not the design.** Stage-0's own exhaustion message was stale the same way — *"reserves 1 GB"*
while reserving 4, which is what a user reads at the moment they hit it. Both fixed.

**This does not make A12 less urgent; it makes the deadline legible.** The reservation is virtual and
resident use is the real limit, which no constant moves. What is new is that the compiler is measured
compiling itself at ~1 GB touched, and there is now a fixpoint test standing between that number and
the next surprise.

**Status of the work itself:** DONE v0.0.261, both stages, per §A7d. The design is unchanged and good — one
`Type::Width { bits, signed }`, boundary-only, a per-width trap. When it lands, `tests/pass/` gets its
first width program, and that fixture is the gate: `the_burxt_backend_compiles_a_growing_share_of_the_
suite` is an `assert_eq!(correct, total)`, deliberately an equality and not a ratchet since v0.0.113, so
the fixture goes red the moment it exists and stays red until **stage-1** implements widths too. It
lands with stage-1's half, in one version, as §A0 requires of every row.

## B — Urgent bugs, silent wrong answers first

| # | Bug | Size |
|---|---|---|
| B1 | **`file_read` of a missing file answers `""`** — indistinguishable from an empty file. The silent wrong answer the thesis exists to refuse, in the standard library. Its own comment says the fix needs `Option`, *"which the language does not have yet"* — **it does** | S |
| B2 | **`os_byte_as_string` is lossy** — every byte ≥ 127 becomes `"?"`. The only int→character path in the library, and it silently destroys data | S |
| B3 | **Hardcoded temp paths** `/tmp/burxt-fs-list`, `/tmp/burxt-os-capture` — two processes clobber each other, and both are a symlink-attack surface on a shared machine | S |
| B4 | **No constant-time compare.** `==` on Strings is `strcmp`, which short-circuits and **leaks the answer through timing** — every token and HMAC comparison | S |
| B5 | **The UTF-8 invariant is declared and unenforced** at all four entry points (`read_file`, `argument`, `os_env`, `c_string_at`). **Decided: validate.** Consequence: binary through `read_file` breaks → needs `file_read_bytes` (A1) | M |
| B6 | **9 builtins are not in `is_reserved_name`** (`bit_*`, `shift_*`, `c_is_null`, `c_string_at`) → a user program can shadow them | S |
| B7 | **Stack overflow is the only failure Burxt does not name** — **re-verified v0.0.247 and still real**: non-tail recursion gives **exit 139, `Segmentation fault`, core dumped**, no named error. **And it bites NARROWER than the row implies, which is worth knowing before someone budgets for it:** a TAIL-recursive runaway does not overflow at all — `return f(n + 1)` becomes a loop under `musttail`, so it runs forever instead of crashing. So the failure mode is a hang for tail calls and a raw signal for everything else, and only the second wants a named error | S–M |
| B8 | A bare **`it` inside a string literal** in a bracket clause is wrong | S |
| B9 | **`lib/json.bx` rejects valid JSON** (`\b`, `\f`, `\uXXXX`). Refuses rather than corrupts, but Burxt cannot read real-world JSON. Needs A5 | S–M |
| B10 | **Iterative AST walkers** — 512 MB stack, ~30k-node ceiling | M |
| B11 | ~~**M7: stage-1 compiles 101 of 102**; generic records in stage-1~~ **CLOSED, v0.0.215 — and it had been closed for a hundred versions without anyone writing it down.** 142 of 142; the one holdout needed `write_bytes`, which landed. Generic records emit in both compilers | — |
| B12 | ~~**stage-1 backend gaps**~~ — **there are none, measured v0.0.215: 142 of 142 pass programs, 0 refused.** The claim came from a stale `M4` §3b. **Re-pointed:** the real gap is the TOOLCHAIN — stage-1 has no LSP, no `burxt review`, no `mcp-schema`, so every non-compiler tool lives only in Rust. Moving one is a genuine capability claim | L |
| B13 | M11's **1.67× compile-time growth is unattributed**; the ratchet tightening is pending | S |
| B15 | **stage-0 accepts a trailing `;` on an interface method signature and stage-1 refuses it.** Found by writing a fixture in v0.0.209 — no existing fixture used the `;` form, so the differential could not see it. A divergence in what is ACCEPTED, which is the direction that matters | S |
| ~~B16~~ | ~~**stage-1 has no C-boundary rule at all, so it ACCEPTS `function f(n: CInt) -> Int` that stage-0 refuses.**~~ **CLOSED v0.0.261.** Found by a13-bytes while scoping A7, verified from the commit rather than the tree: `check.bx` had **0** occurrences of the boundary message against `typeck.rs`'s **3**. `tests/fail/cint_in_burxt.bx` covered only the `let` form, which stage-1 refused *accidentally* through "declared CInt, but the value is Int" — the second process rule in the negative direction, since a fail fixture checks refusal and not the reason. The parameter form never had a fixture, so nothing could see it. Same shape and direction as B15: **a divergence in what is ACCEPTED.** `tests/fail/cint_as_a_parameter.bx` is the fixture that never existed; both compilers now refuse it with byte-identical text | — |
| B17 | **The two compilers agree on the boundary refusal's TEXT and disagree on its SPAN.** Measured at v0.0.261 on `function scaled(n: CInt)`: byte-identical sentence, but stage-0 points at **1:1** and stage-1 at **1:20**; the width form diverges the same way. Neither is wrong-as-such and **stage-1's is the better one** — it puts the caret on the offending type rather than the start of the declaration — so the fix is to move stage-0 to stage-1's span, an improvement rather than a regression. It hides because a `.stderr` fixture records one compiler's output and `the_two_compilers_render_a_problem_identically` compares the rendered message. **The span is not cosmetic**: it is where the editor draws the squiggle and what the LSP returns | S |
| ~~**B18**~~ | **FIXED v0.0.262.** ~~stage-1 emits a bare `sdiv`, so integer division by zero DOES NOT TRAP on arm64.~~ Both divisions and `remainder` now route through guarded helpers in stage-1, with the zero check AND the `INT_MIN / -1` check — the overflow guard was missing from stage-1's `remainder` only — `codegen.rs`'s `int_div_fn` already applied both checks to all three forms, verified by running stage-0 on the fixture before anything was changed, so **no Rust-side change was needed or made**. Stage-1's trio now share one `divide_guard` rather than drifting apart again. Messages are stage-0's word for word, because the differential compares what the two compilers print. And the test that let it through is fixed: `the_burxt_backend_keeps_every_runtime_guarantee` used to accept any non-zero exit — it now requires **exit 70 and the fixture's own message**, so a signal can never again stand in for a guarantee. Verified on x86-64: `divide_toward_zero(7, 0)` from a stage-1-built binary now exits 70 saying *"burxt runtime error: divide_toward_zero(...) by zero"* rather than dying of SIGFPE. Found by the packaging session on the first arm64 runner this project ever used. Original diagnosis: The worst category this list has: a silently wrong number, in division, on a whole architecture. `emit.bx:1936` writes `sdiv i64` for `divide_toward_zero` with no zero check and no `INT_MIN / -1` check; `codegen.rs:5438` builds a real `div_by_zero` block and panics with a named error. **Why 120 versions missed it:** on x86-64 a bare `sdiv` lowers to `idiv`, which *faults* — the program dies of SIGFPE, `the_burxt_backend_keeps_every_runtime_guarantee` sees it die, and counts the guarantee as kept. **The hardware was standing in for the check.** On aarch64 `sdiv` by zero returns 0 and never traps, so the program runs to completion and prints a wrong answer. Measured on `ubuntu-24.04-arm`: *"kept 30 of 32 — `int_division_by_zero` (ran to completion — the check is missing), `int_division_overflow` (ran to completion)"*. **The fix is written two lines below the bug**: `remainder` already routes through `@burxt.remainder` and its comment says why — *"`srem` by zero is undefined behaviour in LLVM — it traps on x86 and the program dies with SIGFPE and no message. A named error beats a signal."* That reasoning was applied to `srem` and never to `sdiv`. **Note what this says about the test, too:** a guarantee test that accepts *"the program died"* cannot distinguish a named error from a signal, and that is what let the hardware answer for the compiler — it should require the named message | S |
| ~~**B19**~~ | **CLOSED v0.0.263.** ~~The two compilers disagree about what a runtime failure is CALLED~~, and nothing compared them — `the_two_compilers_render_a_problem_identically` covers **compile-time** diagnostics only, so runtime text was checked by no test until B18's tightening required the named message. **The same blind spot as B18, one layer up.** Three were wording; `argument_out_of_range` was structural — stage-1 borrowed the ARRAY bounds check, so a program given one argument and asked for its hundredth said *"index 99 is outside an array of 1"*, naming an array nobody wrote. It has its own check and message now. The array message dropped its third number, the **source byte offset** — a compiler-writer's number answering *"where is the expression"* when the reader is asking *"what was I allowed to pass"* — for stage-0's last-valid-index. All four verified byte-identical against stage-0, and mutation-tested: changing one word of one message fails the suite by fixture name. **The exact-match list stays, empty**, because that list is the only thing standing between the two compilers agreeing at runtime and nobody noticing when they stop | — |
| **B20** | **FIXED v0.0.264, both compilers.** The rule generalises rather than matching its repro — a `push` from a nested block inside the region is caught, and a container declared *inside* it is correctly allowed. `tests/fail/push_grows_an_outer_array.bx`. ~~**`push` into an array declared OUTSIDE a `region` is a use-after-free. Confirmed on v0.0.263, silent wrong answer.** `let mutable xs: [Int] = []; region r { push(xs, 11); ... }` then four pushes into a second array, and `xs[0]` prints **777** instead of 11. `push` grows via `build_alloc_array` + memcpy + `store data_p` (`codegen.rs:4278`) — a **fresh arena buffer stored into a binding declared outside the region**, freed by the region's `store next, mark`. Compiles clean, no diagnostic. The escape rule is gated on one site (`typeck.rs:5018`) and only whole-name `Assign` and `Return` have rules. **~100 `push` sites across `tests/pass`**~~ | M |
| **B21** | **FIXED v0.0.264, both compilers**, field and element forms, each with its own wording (`its field` / `its element`). `tests/fail/field_assigned_from_inside_a_region.bx` and `element_assigned_from_inside_a_region.bx`. ~~**Field and index assignment through a `region` is a use-after-free. Confirmed on v0.0.263.** `class Box { name: String }` … `region r { b.name = "hello-" + "world"; }` then allocate a large String, and `b.name` prints **the other string's bytes**. `AssignField` (`typeck.rs:4486`), `AssignFieldIndex` (:4516) and `AssignIndex` (:4569) carry **no region check at all** — not a weak one, none. Same root as B20: the rule was written for whole-name assignment and never extended to the three ways a value reaches an outer place~~ | M |
| **B22** | **FIXED v0.0.264.** `tests/fail/allocates_nothing_through_a_mutable_parameter.bx`. ~~**`allocates nothing` is UNSOUND through a `mutable` parameter — a contract that lies, shipped v0.0.209.** `function fill(mutable dst: [Int], n: Int) -> Int allocates nothing { while … { push(dst, i); … } }` is **ACCEPTED**, and it allocates: `push` calls `burxt.alloc` into caller-owned storage. `burxt explain memory` reports `fill() nothing`. Cause: `push` never asks `has_region()`, and `has_region` is the sole recorder (`typeck.rs:816`), so the owner is never credited. The direct form IS caught, but only because the `let` asks. **For a language whose case is that a reviewer can trust what a signature says, a clause that says "nothing" about a function that allocates is the worst defect on this list** — worse than B20/B21, which are at least honest crashes waiting to happen. M14 §9's acceptance item 6 names three paths — direct, through a call, through a `dynamic` — and this is a fourth nobody wrote~~ | M |
| **B23** | **FIXED v0.0.264.** Stage-1 consults `claims_nothing` now, and the blatant case is refused with stage-0's sentence byte for byte. **The refusal EQUALITY consequently has NO exceptions: 311 of 311** — the three `allocates_nothing_*` fixtures were the last entries in `STAGE_0_ONLY`, excluded on a reason that was wrong twice over. ~~**stage-1 has no `allocates nothing` rule at all — it parses the word and throws it away.** `parser.bx:2530` sets `claims_nothing = 1` and nothing ever reads it. Measured on the blatant case, `function blatant(n: Int) -> Int allocates nothing { let s: String = "x" + to_string(n); return len(s); }`: **stage-0 refuses with a full message; stage-1 says `no errors`.** So B22 is two different bugs sharing a symptom — stage-0 has a hole in a rule that exists, stage-1 has no rule. The wiring is one field away, since `claims_nothing` is already parsed and stored. **Also fix the stale comment above it**, which says *"stage-1 has no such fixpoint — the whole inference is stage-0's"* — false since v0.0.144: `check.bx:4812 infer_allocates` is a full least fixpoint and a two-link unannotated chain is correctly refused. A "NOT DONE is not evidence" case that would send whoever writes slice 3 building something that already exists~~ | M |
| **B24** | **FIXED v0.0.264.** Stage-1 unwinds on all three exits; 200,000 `continue`s out of a region went **13,904 KB → 1,408 KB**, identical to stage-0. Held by a new invariant, `a_region_releases_on_every_exit_from_the_block`, which measures peak RSS in **both** compilers and was mutation-tested by making `close_open_region` a no-op — because the reason this survived is that `tests/pass` compares stdout and nothing that ran a program looked at what it **cost**. ~~**stage-1 does not unwind a `region` on `return`, `break` or `continue`; stage-0 does.** `emit.bx:2297-2303` is the whole of stage-1's region — load the mark, emit the body, store it back — and an early exit branches past the store. Stage-0 has `close_open_region` on all three paths (`codegen.rs:770`, `890-990`). **Measured**, `continue` out of a `region` inside a 200,000-iteration loop, peak RSS: **stage-0 1,408 KB, stage-1 13,904 KB — 9.9×**, same printed answer, and identical the moment the early exit is removed. **This blocks M14 acceptance item 5**, which is an RSS measurement that must hold in both compilers — and it is a prerequisite to slice 3 rather than part of it, because per-block release makes **every block an exit point that must unwind**~~ | M |
| **B25** | **FIXED v0.0.264, both forms** — through a plain function's `mutable` parameter and through a `mutable self` method. Costed **+63 MB of peak RSS** in stage-1, because the second fixpoint walks every body an extra set of rounds and stage-1's arena never releases; that tipped `the_compiler_compiles_itself_without_going_quadratic` and is being recovered by folding the two fixpoints into one set of rounds rather than by raising the bar. **The bar was right and it earned its keep.** ~~**B20 one call away: growth through a `mutable` parameter escapes the escape rule.** Found by testing a shape the fix's author was never shown, after B20/B21/B22 were verified passing. `function grow(mutable dst: [Int], v: Int) { push(dst, v); }` called five times from inside `region r` on an `xs` declared outside it — **accepted, and `xs[0]` prints 777.** The receiver check sees `grow(xs, …)`, not a `push` on `xs`, so the growth is invisible to it. **This is the same shape as B22** — which was "the rule exists, but not through a `mutable` parameter" — appearing in the escape rule instead of in the claim, and it is worth noting the pattern: **a `mutable` parameter is a hole in every rule that reasons about where a value is built**, and there are two such rules. Fix: a second fixpoint answering *"does this function grow, or store into, one of its `mutable` parameters?"*, which can clone `infer_allocates`'s round structure — that fixpoint already exists and already handles cycles. **Still open: the `self` form**, a method growing one of its own fields on a receiver declared outside the region~~ | M |
| **B26** | **FIXED v0.0.267, both compilers.** One arm in `expr_allocates` per compiler, predicate `any_implementation_allocates` — not a flat `true`, which was settled by measurement: flat-`true` refuses `tests/pass/allocates_nothing.bx`, a fixture this project ships. Four fixtures. ~~**A call through a `dynamic` is not treated as allocating, so a value built behind an interface escapes its region. Live use-after-free, reproduced.** `expr_allocates` (`typeck.rs:3421`) has **no `DynCall` arm**, and `typeck.rs:3373` says so in a comment — the `allocates nothing` inference reaches dyn calls by another route (`has_region()`), but the **escape** rule asks `expr_allocates` directly and therefore never sees them. So it leaks through whole-name assignment, `return`, field assignment and element assignment, and one call away through a `mutable` parameter. Proven: `h.tag = d.name()` with `h` declared outside the region prints `item AB`, then **`ZZZZZZZZZZZZZZZZ0`** after a clobbering region. **The `allocates nothing` fixture for the dynamic path is exactly why this looked covered** — one rule consults the dyn path and the other does not, and only the first had a fixture~~ | M |
| **B27** | **FIXED v0.0.267.** One helper, two call sites — the `for` element push and the `match` arm after `bind_arm`. All 16 programs flip. ~~**A binding introduced by a `match` arm or a `for` header carries no taint, and one extra `let` launders any tainted value.** `region_locals` is populated only at `Let`/`Assign`/`AssignField`/`AssignFieldIndex`/`AssignIndex` — **never at a pattern or loop binding.** `match w { Some(s) => { kept = s; } }` inside a region with `kept` outside is accepted; proven, prints `secret-1` then **`ZZZZZZZZZZZZZZZZ0`**. A whole RECORD with a region-built field escapes the same way. **CORRECTED while being fixed, and the correction came from the agent doing the work rather than from me: the `let` relay is NOT broken.** `m5_plain_binding_relay` was already refused, because the `let` site asks `expr_allocates` and its name arm reads `holds_region`, so the taint copies to the new binding. The relay cases leaked because their **roots** carried no taint — a dyn call and a pattern binding — so tainting the roots closed the relays for free, measured. I had recorded this as *"a `let` relay does not restore the taint"*, which is true of the untainted roots and **false in general**; the smaller, correct diagnosis is the one that shipped. (The genuine general laundering route is a **function returning storage reached from a parameter** — a different property, recorded as **B32**.) Found by an audit corpus of 113 programs; **26 of them are wrongly accepted by BOTH compilers**, which is why the differential could not see it~~ | M |
| **B28** | **FIXED v0.0.274 — eleven sites, not the two I found.** Located by running fourteen erroneous generic programs and grepping stage-0's output for `$` rather than by guessing; the last two surface only inside a region or under `allocates nothing`. Also fixed `field_list`, which read `structs` and so printed **`Its fields are: .`** for every instantiation — the empty half of a message whose entire job is to help. Zero mangled names remain in stage-0's corpus output. ~~**stage-0 prints MANGLED generic names in diagnostics where stage-1 prints the source spelling.** `Holder$Int.add(...)` and `grow$Int(...)` against stage-1's `Holder<Int>.add(...)` and `grow(...)`. **Stage-1's is the correct one** — a user never wrote `$` and cannot search for it — so the fix moves stage-0. Two of eight message-text divergences found by the same audit; the verdicts agree, so `the_two_compilers_render_a_problem_identically` does not cover these because they arise in messages no fixture pins. Same family as B17~~ | S |
| **B29** | **FIXED v0.0.266.** `globals` got the chunk list `chunks` and `body_chunks` already had: **1,132 MB → 178 MB**, and 169 with `write_body`'s threshold retuned. Output byte-identical, 5.4× faster. **It also retired H1's argument for A12** — see that row. ~~**Chained string concatenation allocates every intermediate, and it is where the compiler's whole memory footprint lives.** Measured, 50 functions each returning one chain: **5 pieces → 17 MB, 10 → 38, 20 → 119, 40 → 432.** Roughly quadratic in chain length. 601 lines of 40-piece chains peaks at **4,095 MB**, against a 4 GB arena — a 601-line program nearly exhausts the compiler. **This is 95.7% of stage-1's cost on its own source**: checking `main.bx` takes **49 MB** and checking-plus-emitting takes **1,132 MB**, because `emit.bx` builds the runtime's IR with chains dozens of pieces long. Simple code costs ~6 KB/line; `main.bx` costs **61.6**, and the difference is chains. **This is §D0's `Builder` question, which that section says has already been paid for three times** — but as a LANGUAGE defect rather than a library one: `a + b + c + d` should allocate the result once, not three times, and every Burxt program pays this today. **It also settles what A12 can and cannot do**: the memory is GARBAGE rather than live data, so per-block release genuinely would reclaim it — but folding a chain into one allocation is far cheaper, helps every program, and does not need escape analysis to be correct first~~ | M |
| **B30** | **FIXED v0.0.268.** Both interning loops chunk now, through one shared `join_chunks`. Measured: **2,000 B 14→8 MB, 4,000 B 31→9 MB, 8,000 B 100→9 MB**, and a **32,000-byte literal costs 14 MB** — flat where it was quadratic. IR byte-identical on all 159 fixtures. `tests/pass/a_large_string_literal.bx` keeps a 4,000-byte literal in the suite, because the cliff survived precisely by **no fixture having a big one** — it was under every user and none of us. ~~**Escaping a string literal to IR is quadratic in that ONE literal's length — a user-facing cliff, not a self-hosting one.** `body += hex_of(...)` at `emit.bx:343` and `emit.bx:396` builds the hex escape a byte at a time. Measured: 2,000 B → 14 MB, 4,000 B → 32 MB, **8,000 B → 103 MB for a single literal**. The compiler's own longest literal is 223 bytes (`emit.bx:1287`) so it costs nothing today, which is exactly why it went unseen — but **any user program embedding a 10 KB SQL schema, HTML template or JSON fixture pays ~150 MB for it**, and the shape of program that does that is the shape a real application is. Same fix as B29: a chunk list rather than a flat append. Found while attributing B29, and worth more than B29 in the long run, because B29 only ever hurt us~~ | S |
| **B31** | **FIXED v0.0.268.** The claim is deleted and the arithmetic kept. The measurement was real and attributed to the wrong cause: that 600 MB was **B29**, and this threshold is very nearly inert because `close_function` already hands it 512-byte pieces. ~~**`write_module`'s chunk threshold is inert, and its comment credits it for 600 MB it never saved.** `emit.bx:60-66` claims *"T = 4096 spent about 600 MB and ran the region out; T = 512 spends about 95 MB."* Swept 64→4096, the total moves by **3 MB** (182.0 / 182.2 / 182.3 / 180.6 / 185.0). The reason is that `close_function` already feeds it 512-byte pieces, so its appends are coarse and the threshold almost never fires. **That 600 MB was B29's `globals` being blamed on this constant** — a measurement attributed to the wrong cause, written down confidently, and then reasoned from for many versions. Delete the claim and keep the code; the tuning that DOES matter is `write_body`'s, which went 512 → 128 for −9 MB in v0.0.266~~ | XS |
| **B32** | **FIXED v0.0.269, both compilers, and it closes the whole family.** A fourth call-graph property — *may this function's return value alias one of its parameters* — riding in the rounds `infer_allocates` already runs, so **peak RSS is unchanged at 169 MB**. Corpus: **126 correct on both, 0 wrongly accepted, 0 false positives, and ZERO verdict divergences** — from 81/45/0/5 when the audit started. 173 of 173 real programs still accepted, and the precision holds where it matters: a callee returning a literal, an Int, or a freshly built String does not alias its argument and is still accepted. `tests/fail/a_relay_function_carries_region_storage_out.bx`. ~~**A function that returns one of its parameters launders the taint — 18 corpus programs, and it is the GENERAL form of B20/B21/B25/B26/B27.** `function pass(s: String) -> String { return s; }` then `kept = pass(built)` from inside a region: **accepted, and `kept` prints `secret-value` and then `0` once the memory is reused.** The escape rule asks *"does this expression allocate?"* — and `pass(...)` does not allocate, it returns something already built. **The question it should be asking is "does this value point into the region", which is aliasing, not allocation.** Every earlier hole was one construct missing from an enumeration; this one says the enumeration is the wrong shape. Variants in the corpus: an identity relay, a **generic** identity relay, a field getter, a relay chained twice, a relay selecting between two parameters, a relay to a field or element of a `mutable` parameter, a bare argument assigned outward — and **`?` launders too** (`L1_try_launders`). Needs a fourth call-graph property, *"may this function's return value alias one of its parameters"*, which is the same fixpoint shape as the other three and can share their rounds. **Does not block B26/B27**, which close 108 of 126 with zero false positives and are a strict improvement~~ | L |
| **B33** | **FIXED v0.0.267**, and the lying comment corrected with it. ~~**`argument(n)` escapes a region in stage-0, because a comment in the checker contradicts the codegen it describes.** `typeck.rs:5855` reasons *"No region: the C runtime's argument strings outlive the program, so this borrows rather than copies."* `codegen.rs:3831` says the opposite, in capitals: *"**COPIED into the region**, with a header."* **The codegen is right** — `argv` holds C strings with no length header, so handing one back directly would make `len` read whatever the loader put before it, and the copy is deliberate. So `kept = argument(0)` inside a region is accepted, and proven to corrupt: `len(kept)` prints **22, then 1** after a clobbering region. **Stage-1 correctly refuses it**, so this is also a verdict divergence in the direction that matters — the Rust compiler is the permissive one. Found by enumerating all 41 `TypedExprKind` variants against `expr_allocates`'s match arms and checking every one that falls through to `_ => false`~~ | S |
| **B34** | **FIXED v0.0.267 in stage-0, which is where the hole was** — stage-1 has had a kind-22 arm all along, with a comment explaining it. Only stage-0 needed the arm; v0.0.267's message was loose about that and v0.0.268 corrected it. ~~**`?` (`Try`) is a third `expr_allocates` gap, alongside `DynCall` and `Arg`.** The operator unwraps an `Ok` payload and drops the taint with it, so a region-built value reaching an outer binding through `?` is accepted by stage-0 and refused by stage-1 — another divergence where stage-0 is the permissive one. **Found by the method rather than by luck**: all 41 `TypedExprKind` variants enumerated against the match arms, every `_ => false` fall-through checked. Exactly three can carry region storage — `DynCall` (B26), `Try` (this), and `Arg` (B33) — and `DynCoerce` turns out unreachable because a `dynamic` must come from a variable. **That enumeration is the durable part**: it is the difference between "we fixed the cases we thought of" and "we know which cases exist", and it is why this list can now be closed rather than extended~~ | S |
| **B35** | **FIXED v0.0.267**, together with B26 rather than after it, because it is a third missing arm in the same function — and landing them apart is what briefly created a divergence. ~~**Reading a FIELD or an ELEMENT of region storage launders the taint in stage-1 — and stage-0 has always refused it, so the two compilers disagree about whether a use-after-free is a program.** No pattern, no `dynamic`, no relay: a plain `let`, then `h.tag = b.name;` where `b` was built in the region and `h` was not. Verified: **stage-0 refuses with the existing field sentence; stage-1 accepts**, and the program prints `ab` then `0` after a clobbering region. The element form (`kept = made[0]`) is the same. Cause is the same function and the same shape as B26 — `expr_allocates` at `parser.bx:1216` has no arm for a field access or an index — so it is a **third missing arm in the place the B26 arm goes**, closed with it. **This is the worse category**: a shared leak is invisible to the differential because both compilers agree, but this is a *disagreement* the differential exists to catch and could not, because no fixture spells it. It also bounds B27 — taint reaches a `for` binding correctly and is then thrown away by the field read — and **stage-0's B26 fix landing before this arm CREATED a fresh divergence** on `for it in made { h.tag = it.name; }`, which is a sequencing error of mine: I let one compiler move ahead of the other on the same rule~~ | S |
| **B36** | **FIXED v0.0.272, both compilers**, gated on the corrected `may_be_region_storage`. The four-row boundary held exactly: an Int element and an Int field are **accepted and print the right value**; the whole `[Int]` and a record with a String field **stay refused**. A second false refusal fell out with it — `return b.n` out of a region. Nine fail and five pass fixtures. ~~**The field/index arm is TYPE-BLIND, so reading an `Int` field of a region-built record is now refused — a false refusal, taken deliberately and declared rather than discovered.** `class Pair { label: String, n: Int }` built inside a region, then `total = b.n` with `total` outside: **both compilers refuse it**, though an `Int` is copied by value and could not dangle. Same for `made[0]` on an `[Int]`. **Introduced knowingly by B35's arm and reported by the agent that wrote it**, which is the reason it is a row rather than a bug report from a user later. The trade: the arm asks *"does the thing you reached through hold region storage"* without asking what the reached-for thing IS, and **stage-0 refused this before today as well** (measured against the frozen pre-fix binary), so closing it in stage-1 made the two compilers agree conservatively rather than making one of them cleverer alone. That is the right direction under M14 §2 Decision 2 — *when in doubt, promote outward; a wrong guess must cost memory or ergonomics, never correctness*. **Narrow it in BOTH compilers or neither**, and the arm's comment says so. `len(xs)` on a region-built array is correctly accepted, so builtins do not propagate the taint~~ | S |
| **B37** | **FIXED v0.0.272.** A `check_return_storage` pass in `check.bx` between `infer_allocates` and `check_bodies`, with a `region_allocated` predicate mirroring stage-0's, and the message copied out of `typeck.rs` byte for byte. **Ordering is the rule, not a detail** — `complain_at` keeps only the first problem, so it must run before the bodies. `allocates` still excuses it. It also closes L15's text divergence: stage-1 now refuses for stage-0's reason instead of the relay rule's. ~~**Stage-0 refuses `function f(xs: [String]) -> [String]` outright and stage-1 accepts it — a whole RULE stage-1 does not have.** Verified with **no `region` anywhere in the program**, so it is independent of the escape family: stage-0 says *"function `same` cannot return [String], because its storage lives in a region and would not outlive it"*, firing on the **declaration** at 1:1; stage-1 says nothing. Found while scoping B32, because `L15_relay_returns_a_slice_param` is refused by stage-0 for this reason rather than by the relay rule — so when B32 lands, the two compilers will **agree on the verdict and differ on the text**, stage-1 naming the binding with the whole-name sentence. That divergence is **created knowingly and is the right trade**: a verdict agreement plus a text difference beats today's ACCEPT, and closing it properly means giving stage-1 the missing rule, which is this row and not B32's job. **Note the direction** — stage-0 is the stricter one here, which is the opposite of B33/B34 and worth holding in mind: neither compiler is reliably the conservative one~~ | S |
| **B32b** | **FIXED v0.0.270**, four shapes, seven fixtures. ~~**v0.0.269 claimed the escape family was closed and it was not — four relay shapes shipped with stage-1 accepting what stage-0 refused.** A relay that wraps its argument in a **record literal**, one through a method's **ordinary parameter** rather than its receiver, one through a **`dynamic`** (the arm asked whether any implementation ALLOCATES and returned, never going on to ask whether any RELAYS), and one returning an **element of a parameter array**. All four proven use-after-frees, all four stage-1 permissive. **FIXED v0.0.270**, four fixtures added. **The corpus reported zero divergences on the broken commit**, which is the finding: a differential is only as wide as its corpus, and 133 adversarial programs written by the person who also could not think of these shapes is not the language. Also a coordination lesson — two agents on one property agreed on the diagnostics and not on the SCOPE, and the one that saw the gap treated *"closing it would create a divergence"* as a reason to stop rather than a reason to ask, while the other half was still soft~~ | S |
| **B38** | **ACTED ON v0.0.271** — the seven probes are fixtures now. Kept as a standing row rather than closed, because it is a property of every corpus this project will ever build, not a defect that was repaired. ~~**A corpus that scores 100% has stopped being evidence — measured, not asserted.** After B32b, the 133-program audit corpus reports **126 of 126 correct, zero divergences**. It reported *exactly that* at `e1aaccf`, while stage-1 accepted four proven use-after-frees. And under mutation it is worse than merely quiet: **each of B32b's four arms can be deleted individually and the corpus still says 126 of 126.** Everything holding those arms is a handful of hand-written probes. The corpus is now a **regression net, not a detector**, and the distinction is the whole lesson — it can prove nothing broke, and it cannot find what nobody thought of, because the programs in it were written by the same people with the same blind spot. **Acted on rather than only recorded**: the probes are now fixtures (`a_relay_h1`–`h4`, `a_dynamic_relays_a_parameter`, `a_relay_through_an_enum_payload`, and the accepting `a_dynamic_relay_of_untainted_storage`). **The accepting fixtures carry the weight** — every corpus program that should be refused stays refused whether the property is precise or wildly over-refusing, so only the programs that must still COMPILE can tell those apart~~ | — |
| **B39** | **FIXED v0.0.272 in stage-0, and my model of it was wrong twice — the second correction came from mutation.** In **stage-0 both repros come through the `Named` arm only**: `expand` rewrites every *concrete* generic application to `Named("Wrapper$Int")` before any body is checked, so the `Generic` arm never meets one, and a non-concrete application has a `Param` argument that the arguments-only test already answered `true` for. Measured by restoring each arm separately: **`Generic` restored → still refused; `Named` restored → runs and corrupts, and `lib/json.bx` truncates.** So *"fix one arm and the other stays open"* — which I wrote into this row and sent to two agents — was **not true of stage-0.** The `Generic` fix is kept as defence in depth (it is the correct answer, costs nothing, and its absence would be silent the day `expand` stops covering a case) and is **explicitly not claimed as measured**. Whether it is load-bearing in stage-1 is that compiler's own measurement — it may not monomorphise at the same point. Three things done beyond the fix, all in the bug's own direction: an **unresolvable `Named` now answers YES**, because "unresolvable answers no" is exactly how both holes stayed invisible; the **`_ => false` catch-all is gone**, replaced by the seven scalar variants by name, so a `Type` variant added by a future milestone cannot inherit "holds nothing" in silence; and a **`seen` guard**, since following a declaration can cycle. **`could_hold_storage` deleted** — and *why* it could go is the best argument on this page against keeping a second answer to one question: it was written **because** the shared predicate was defective, and it had **the identical `Generic` hole**, escaping only for the same reason the original does. ~~**`may_be_region_storage` has TWO holes, in two arms, and which one you hit depends on whether the type is still generic or already monomorphised.** `Type::Named(n)` looks up `structs`/`enums` only, so an instantiation like `Wrapper$Int` — which lives in `made_records`/`made_enums` — misses and falls to `false`. `Type::Generic { arguments, .. }` asks only the type ARGUMENTS, so a generic whose DECLARATION has a `String` field answers "not region storage".** `class Wrapper<T> { t: T, note: String }` — every argument of `Wrapper<Int>` is an Int, so the predicate says no, and a rule gated on it lets the value out. Proven by porting it into stage-1's field/index arm: the program **ran and printed the next region's bytes twice**. **Stage-0 refuses the program today**, so there is no live bug — the hole is a trap that springs the moment either compiler narrows a rule with this predicate, which is exactly what B36 asks for. **Found independently from two directions in the same hour**: the stage-1 agent by porting it, and the stage-0 agent from the other side — its `could_hold_storage` deliberately does not reuse this predicate because a generic INSTANTIATION resolves through `made_records`/`made_enums` rather than `structs`/`enums`, so it answers "no" for `Result$Json$String`, and **`lib/json.bx` printed a truncated document** until that stopped being believed. **Three agents found this independently in one hour, each through a different arm and a different symptom** — one porting the predicate and watching a use-after-free run (Generic arm), one building A12 and having a released block corrupt `lib/json.bx`'s output (Named arm), one reviewing (Named arm again). Their reports read as disagreements — *"it is the Generic arm"* against *"it is the Named arm, not Generic"* — and **both were right about different arms**, which is only visible by reading the function rather than arbitrating the accounts. Fix BOTH: `Named` must also consult `made_records`/`made_enums`, and `Generic` must ask the arguments AND fall through to the declaration's fields, where a field of type `T` is a type parameter and answers yes on its own~~ | S |
| **B40** | **stage-1 accepts a mutual containment cycle that stage-0 refuses, and emits 24,780 bytes of IR for it.** `class A { b: B }  class B { a: A }` — stage-0: *"a `A` cannot contain a `A` — it would have no finite size"*; stage-1 compiles it. Stage-1's check catches only **direct** self-containment, not a cycle through a second class. Measured on v0.0.271: `check` exits 0 and the emit path produces real IR for a type that cannot have a size. The agent that found it saw stage-1 **spin until killed** on its variant, so the failure mode may be a hang or a nonsense layout depending on the shape — either way stage-1 is the permissive side on a program that cannot exist | S |
| **B41** | **FIXED v0.0.274.** ~~**The two compilers' "cannot return an array yet" messages differ in the last clause.** `function same(a: [Int; 3]) -> [Int; 3]` — stage-0: *"Return a class, or fill an array the caller owns."*; stage-1: *"Return a class **holding it**, or fill an array the caller owns."* Both refuse, so the differential's verdict check is silent; it is the text that differs. B17 family~~ | XS |
| **B42** | **FIXED v0.0.272 in stage-0** by the `Named` arm; stage-1 already refused it. ~~**B39's predicate hole is not only a trap — it is a LIVE use-after-free in stage-0 today, through B27's taint rule.** A `match` binding whose payload type is a generic INSTANTIATION is never tainted, because `may_be_region_storage` resolves a `Named` type through `structs`/`enums` while instantiations live in `made_records`/`made_enums`, so it answers "no" for `Wrapper$Int` and every other one. Proven, accepted by stage-0 and run: `enum Holder<T> { Full(T) }` carrying a `Wrapper<Int>` whose declaration has a `note: String`, matched inside a region and assigned out — prints `secret-value`, then the clobbering region's bytes **twice**. **Stage-1 REFUSES it**, because the agent porting the predicate fixed the Generic arm first — so stage-0 is the permissive side on a live use-after-free, which is the worst combination on this list. Predicted by the agent building A12 from the shape of B39 rather than found by a fixture: *"B27's taint rule is reading the same wrong answer"*. It was right, and the corpus had no program for it~~ | M |
| **B43** | **`==` on an enum: stage-0 refuses it, stage-1 accepts it.** `enum Shade { Dark, Light }` then `a == Shade.Light` — stage-0 says *"`==` on the enum … is not available yet"*, stage-1 compiles it. **No region anywhere**, so it is nothing to do with the escape family — it was **unmasked** by B36's narrowing, which had been over-refusing region-shaped programs and hiding it. Stage-1 is the permissive side | S |
| **B44** | **Whole-array assignment of a `[Int; 3]` field: stage-0 refuses, stage-1 accepts.** `kept = h.xs` — stage-0 says *"whole-array assignment is deferred"*, stage-1 compiles it. Also region-free, also unmasked by B36's narrowing. Stage-1 permissive | S |
| **B45** | **FIXED v0.0.274** — the index term is **dropped outright**, not gated. Gating was not enough: with a `[String]` element it fired again. ~~**stage-0's index arm keeps a disjunct that was reported as needing removal, and it is a false refusal.** `expr_allocates(index)` is still OR'd in — moved inside B36's gate rather than dropped — so `kept = xs[idx()]` with `xs` built OUTSIDE the region and only the *index* allocating is **refused by stage-0 and accepted by stage-1, and stage-1 is right**: nothing the index does can make the element region storage. Predates B36. **Deliberately NOT mirrored into stage-1**, because importing a false refusal to obtain agreement is the wrong direction — recorded in the arm's comment so the difference does not read as an oversight~~ | S |
| **B46** | **The suite cannot see a span regression at all.** Not one `tests/fail/*.stderr` golden contains a caret line — they hold message text only, so a caret can move anywhere and every fixture still passes. `every_rejection_reports_a_position_that_points_at_code` checks that a position points *at code*, never how far it extends. **That is why B17 drifted for months**, and the only thing that caught it is the audit corpus's span column, which is not in CI. Same shape as the second process rule: a fixture set cannot tell "correct" from "nobody wrote the case" | S |
| **B47** | **A method returning a fixed array: stage-0 refuses, stage-1 accepts silently.** `function (self: Box) same(a: [Int; 3]) -> [Int; 3]` — B37's shape in the method spelling, which B37's fix did not cover. Stage-1 permissive | S |
| **B48** | **`unknown function: nofunc` against `unknown function`** — stage-0 names the function and stage-1 does not, so this divergence moves the OTHER way: stage-1 adopts stage-0's. Worth noting because every other text row this week ran stage-0 → stage-1 | XS |
| B14 | **Doc rot** — `lib/README.md` claims `Option`/`Result` do not exist · `map.bx` claims no bit ops · the module table omits 3 modules · `docs/reference/builtins.md` omits 9 builtins while claiming to be generated from that list | XS |

---

## C — The rest of the 1.0 bar

| # | Item | Size |
|---|---|---|
| C1 | **DWARF debug info + an `-O0` flag.** Stage-0 only for 1.0, stated as such. Matters because *an agent that cannot debug inserts `print`, which moves the stack and changes the answer* — the v0.0.141 trap | M |
| C2 | **Dependency management** — manifest (git URL + tag), lockfile, local cache, `pub`/visibility. **No registry for 1.0.** `burxt review` becomes the semver rule: a major bump is *mechanically detectable*, which nothing else can do | L |

---

## D — The standard-library floor: full Rust `str` + `Vec` parity

**D0 comes first, before a single function is written:** decide the **`Builder`** shape. `out = out + b`
is O(n²) and this project has paid for that three times (v0.0.68, v0.0.77, v0.0.82). Every one of the
~100 functions below must be run-based, or the library ships 80 quadratic appends.

### D1 — writable today, no compiler change

| # | Module | Functions |
|---|---|---|
| D1a | **`lib/string.bx`** transform | `to_upper_ascii` · `to_lower_ascii` · `capitalise` · `title_case` · `equals_ignore_case` · `find_ignore_case` · `compare_ignore_case` · **`replace`** · `replace_first` · `reverse` *(needs A5)* · `pad_start` · `pad_end` · `pad_centre` · `trim_start` · `trim_end` · `trim_bytes` · `strip_prefix` · `strip_suffix` · `slice` *(end-exclusive)* · `insert` · `remove` · `shorten` · `squeeze_space` · `indent` · `dedent` |
| D1b | **`lib/string.bx`** search & split | `rfind` · `find_from` · `count` · `find_any` · `rfind_any` · `contains_any` · `find_at -> Option<Int>` · `split_space` · `split_any` · `split_times` · `rsplit` · `split_once` · `split_inclusive` · `split_no_empty` |
| D1c | **`lib/string.bx`** classify & compare | `is_empty` · `is_blank` · `is_digit` · `is_alpha` · `is_alnum` · `is_upper` · `is_lower` · `is_punct` · `is_hex_digit` · `is_ascii` · `all_digits` · `all_alpha` · `compare` *(3-way)* · `compare_natural` · `common_prefix_len` · `edit_distance` |
| D1d | **`lib/string.bx`** parse & format | `parse_int_base` · `parse_hex` · `int_to_base` · `int_to_hex` · `int_to_binary` · `int_padded` · `int_grouped` · **`parse_decimal`** *(per scale)* · `decimal_padded` |
| D1e | **`lib/array.bx`** | **`slice`** · `copy` · `concat` · `insert_at` · `remove_value` · `count_of` · `last_index_of` · `binary_search` · `equals` · `dedup` · `product_int` · `repeat` · `take` · `drop` · `pop` *(precondition form)* · `rotate` · **a faster stable sort** |
| D1f | **`lib/map.bx`** | **`values()`** · **`entries()`** · `clear()` · `merge()` · `take()` · `is_empty()` · `map_increment` · `map_from` |
| ~~D1g~~ | ~~**`lib/set.bx`**~~ **DONE v0.0.251** — 367 lines, `class Set<T: Equatable>` over `Map<T, Bool>` with `add`, `add_all`, `has`, `remove`, `take`, `count`, `items`, `is_subset_of`, `equals`, `union`, `intersect`, `difference`. **`take() -> Option<T>` is the §D2a item, and it was unwritable this morning** — an Option-returning generic needed A3, which landed hours earlier. Reads under both compilers |
| ~~D1h~~ | ~~**`lib/math.bx`**~~ **DONE v0.0.249** — 528 lines, 24 declarations, `INT_MAX`/`INT_MIN` as folded consts, and all three `checked_*` are `pure`. **The overflow ORDER is the design:** Burxt's `+` traps, so `checked_add` cannot compute-then-test — it asks `math_add_overflows` first, which is why those three predicates exist as public functions rather than hiding inside. Measured: `checked_add(INT_MAX, 1)` answers None without crashing, `isqrt(15)` is 3 and `isqrt(16)` is 4 exactly. Reads under both compilers |
| D1i | **Decimal helpers** | per-scale `abs` · `min` · `max` · `is_zero` · `percent_of` · `round_to` · **`money_split`** — largest-remainder penny allocation, *the* canonical exact-money problem, and absent |
| ~~D1j~~ | ~~**`lib/time.bx`**~~ **DONE v0.0.255** — 534 lines, `DateTime` and `Duration`, Hinnant's `days_from_civil`/`civil_from_days` (exact integer arithmetic, no tables), ISO-8601 format and parse, `weekday`, `day_of_year`, `is_leap_year`, `days_in_month`. **UTC only and it says so**, per `DESIGN.md`'s commitment that *"dates/timezones, when they come, arrive timezone-explicit or not at all."* Verified on the cases that catch a subtly-wrong date library: **1900 not leap, 2000 leap** (the pair that catches a wrong century rule), 1970-01-01 = day 0, 1969-12-31 = **-1** so pre-epoch works, `1700000000` → `2023-11-14T22:13:20Z` round-tripping, and **`2024-02-30` parses to None** rather than being accepted. Monotonic and sub-second still need A7, and that limit is named rather than approximated |
| D1k | **`lib/random.bx`** *(new)* | seeded xorshift/PCG · `next_below` · `next_between` · `shuffle` · `choice`. **Named `random_from(seed)`, never a bare `random()`** — reproducible on purpose, wrong for keys |
| D1l | **`lib/path.bx`** *(new)* | `join` · `basename` · `dirname` · `extension` · `stem` · `is_absolute` · `normalise`. **None exist at all** |
| D1m | **`lib/files.bx`** | `read_maybe -> Option<String>` · `is_directory` · `is_file` · `size` · `copy` · `remove_directory` · `walk` · `read_bytes` *(A1)* · `temp_file` |
| D1n | **`lib/log.bx`** *(new)* | `debug`/`info`/`warn`/`error` · `BURXT_LOG` threshold · stderr · timestamps. Closes the audit's `structured logging: Blocking` |
| D1o | **Errors** | `assert_that(held, why)` · `panic(why)` · `todo()` · `unreachable(why)` · `result_is_error` · `result_context` · `option_ok_or` |
| D1p | **UTF-8 layer** *(A5)* | `next_char` + `CharAt` · `char_count` · `char_at` · `char_index` · `from_codepoint` · `codepoint_at` · `is_valid_utf8` · `is_continuation` · `from_byte` *(retires the lossy `os_byte_as_string`)* · `to_bytes` · `from_bytes` |
| D1q | **Process / env** | `os_capture_status` — stdout, stderr and exit code separately; today they are merged with `2>&1` · `os_set_env` · `os_pid` · `os_cwd` · `os_platform` |
| D1r | **`sleep(ms)`** | A five-line extern. **Blocks every retry and poll loop today** |
| D1s | **`range(n) -> [Int]`** | Stopgap until A6 |
| D1t | **`lib/csv.bx`** *(new)* | read + write. JSON is covered thoroughly; CSV is the other universal interchange format, and the one a money language is handed most |

### D2 — needs A first, listed so nothing is written twice

| # | Item | Needs |
|---|---|---|
| ~~D2a~~ | ~~`array_pop<T> -> Option<T>` · generic `Set` · `map.take` · `option_ok_or`~~ **A3 UNBLOCKED v0.0.241 and the payoff is cashed:** `array_pop<T>` measured working in both compilers, and `lib/set.bx`'s `take() -> Option<T>` shipped v0.0.251 — a generic Set with an Option-returning method, which is the whole row | ~~A3~~ done |
| D2b | Codepoint-correct `string_reverse`, case handling, char indexing, JSON `\u` | A5 |
| D2c | `zip` · `enumerate` · `char_indices` · `split_at` · `divmod` | A8 |
| D2d | `map` · `filter` · `fold` · `any` · `all` · `sort_by` · `retain` · `partition` · `position` | A9 or A10 |
| D2e | Monotonic clock · sub-second time · benchmarking · timeouts | A7 |
| D2f | `chunks` · `windows` · borrowed sub-slices | a slicing decision |

### The two omissions most embarrassing for the pitch

Both are in D1, and both deserve naming here so they are not deprioritised as "just more functions":

1. **You cannot read a `Decimal` out of a file.** There is no `string_parse_decimal`, in a language
   whose headline feature is exact money. Reconstruction via `tick * count` works but must be written
   once per scale, because a scale cannot be a type parameter.
2. **`money_split` does not exist.** Splitting $100.00 three ways so the parts sum *exactly* to the
   total is the canonical exact-money problem, it appears in no library or example, and it is the first
   function a reader of the pitch would look for.

---

## E — Security and cryptography: build vs bind

Verified: **zero** occurrences of sha256, hmac, aes, encrypt, bcrypt, argon, base64, jwt, constant-time
or urandom anywhere in `lib/`, `src/` or `examples/`. The only checksum in the project is CRC-32, and it
lives in a **test fixture**.

**Why this needs its own section.** A language whose stated purpose is *"an agent writes the code, a
senior developer reviews it"* has a specific exposure: **if the primitives are absent, an agent will
hand-roll them** — and a subtly wrong AES produces ciphertext that looks perfectly fine. The absence is
not neutral; it invites the exact outcome this language exists to prevent.

**So the line is drawn by testability, not by difficulty.**

| # | Item | Verdict |
|---|---|---|
| E1 | **SHA-256 / SHA-512 · HMAC · PBKDF2** | **BUILD** — published test vectors, no secret-dependent branching. Verifiable exactly as CRC-32 already is, so "it compiles" and "it is correct" become one statement |
| E2 | **hex · base64 · base64url** encode/decode | **BUILD** |
| E3 | **CSPRNG + `uuid_v4`** | **BUILD, after A1.** Impossible today |
| E4 | **Promote CRC-32** out of `tests/pass/bits.bx` into `lib/hash.bx`; add `fnv1a` for a version-stable hash | **BUILD — already written and verified. Cheapest win on the board** |
| E5 | **AES · ChaCha20 · RSA · Ed25519 · X25519 · TLS · Argon2/scrypt/bcrypt** | **BIND — do not hand-roll.** Two reasons: no control over instruction timing, and RSA and the curves need **arbitrary-precision integers**, which do not exist — `Decimal` is a scaled i64 capped at scale 18 |
| E6 | **Secrets cannot be zeroed** — a String lives until its region closes; there is no `zeroise` | **document as a 1.0 limitation** |

**Two naming decisions, to make with the modules rather than after:** `random_from(seed)` never a bare
`random()`, and `string_equals_constant_time` spelled out in full — for the same reason `divide_floor`
and `shift_right_zeros` are, because the *behaviour* is the point.

---

## F — Papercuts

Stack trace on failure · reach a Decimal's unscaled integer · `to_string` of a record / a display trait ·
default parameter values and named arguments · `burxt fmt` · ~~`==` on records and enums~~ (**records DONE — measured v0.0.247: `P{x:1,y:2} == P{x:1,y:2}` is `true`, `!=` works, and a differing field gives `false`. Enums untested. Another ☐ that was stale, the second this stretch after A1**) · nested match
patterns *(trigger fired v0.0.118)* · `old(...)` of an aggregate and `ensures` on an aggregate return ·
`pure` methods · `decreases` on methods · mutual recursion and lexicographic measures · `if` as an
expression · `allocates` on methods · `[0; N]` literals · unit literals (`5.km`) · pipelines ·
attributes `#[...]` · regex · editor go-to-definition and a tree-sitter grammar · `List<T>` as a library
type · no warnings only errors · parser errors arrive alone · SOLID lints · stage-0 AST renames · the
`region` naming sweep · profiler · compound `Map` keys.

---

### F13 — `tail` is a reserved word, and it is the only common NOUN among them (found v0.0.259)

`let tail: Int = 5;` does not compile: *"expected identifier after 'let', found `tail`"*. Measured
v0.0.259, and found the only way it could be — by writing the compiler's own test suite **in Burxt**,
where a list's `tail` is the obvious name for a thing.

**The full reserved list is 31 words**, and every one of them is a structural keyword or a verb except
this: `as break class const continue dynamic else enum external false for function if implement
implements in interface is let match mutable print print_error private pure region return self tail true
while`. Checked against the words a user would plausibly reach for: `result`, `old`, `it`, `value` and
`count` are all **usable**; `self` and `match` are reserved and nobody is surprised. `tail` is the
outlier, and it is a noun — `head`/`tail` in list code, `tail` of a file, `tail` of a queue.

**Why it is reserved at all, and why the fix is not free.** `tail` appears only in `return tail <call>`,
which sits in a position an ordinary identifier can also occupy. `return tail;` is unambiguous today
because it cannot mean a tail call (there is nothing to call) — but `return tail (x);` genuinely is
ambiguous: a tail call of `(x)`, or a return of `tail(x)`, a call to a function named `tail`. So making it
contextual needs two tokens of lookahead and still leaves that case, which is exactly the kind of rule
`spec/A7.0-NAMING.md` warns costs more to explain than it saves.

**Three options, none obviously right, so this is recorded rather than decided:**

1. **Leave it.** Cost: a common noun is unavailable, forever, and every user meets it the first time they
   write a list function. Cheapest, and the honest note is that `head` being free while `tail` is not is
   the sort of asymmetry that reads as an accident even when it is not.
2. **Contextual after `return`**, accepting the `return tail (x)` ambiguity by resolving it one way and
   naming the resolution. Costs lookahead and a rule to remember.
3. **Rename the feature** — `return musttail`, or fold it into `decreases`' vocabulary — so the reserved
   word is one nobody wants. Costs a language change in both compilers plus the grammar/`.vsix`/reference
   cascade (§A7d), for an ergonomic win.

Size **S** for 1, **M** for 2 or 3. Not urgent, and it belongs to whoever decides the tail-call surface.

## G — Post-1.0, by gate

The grouping is the useful part: these are not independent items, they are five gates.

| # | Gate |
|---|---|
| G1 | **Concurrency** — threads, shared regions, **derived mutual exclusion from a declared invariant** (*"the genuinely novel step"*), data races as compile errors, `map_seeded`. Regions were chosen partly to make this right; effect handlers are the intended mechanism. **Its first named consumer is [M15](M15-WEB.md) W3/W4** — a listening server needs this and nothing weaker, and both cheap substitutes were offered and **refused** (Andre, 2026-08-01): a serial accept loop is not a server, and `fork()` per connection makes sharing impossible rather than safe, which is the opposite of what this gate is for |
| G2 | **The pointer wall's remaining doors** — callbacks into Burxt (→ `sqlite3_exec`, `signal`), C→Burxt strings, an environment effect → then sockets → TLS → HTTPS → a model client, **and the web stack ([M15](M15-WEB.md) W2/W5)**. The socket step is narrower than this row reads: `socket`/`send`/`recv`/`listen`/`close` already cross, because a fd is an `int` (measured, `NOVELTY.md` §8) — only `bind`/`connect`/`accept` wait, on the C struct layouts A7 unblocked in v0.0.261 |
| G3 | **M3 packaging** — per-target linking, desktop matrix, Android NDK/JNI, iOS signing, wasm host glue. *Objects already emit for **13** triples with byte-identical IR; what remains is a sysroot per platform.* Was written as 8; the other five (Android's three ABIs, `aarch64-apple-ios`, `wasm32-wasi`) already worked and nothing had looked — measured and added to `the_ir_is_the_same_for_every_target` in v0.0.260. **G3 is target-side and stays post-1.0**; the HOST work (four platforms, the image) shipped in §H without needing a sysroot, and reading G3 as "packaging is post-1.0, so macOS waits" has the split backwards — see [ROADMAP-1.1](ROADMAP-1.1.md) §G3 |
| G4 | **Freestanding runtime (IoT)** — configurable region, no-libc mode, `print` routed out. Xtensa (ESP32), AVR, MSP430, ARM/Thumb and RISC-V 32 backends are already registered and `armv7` emits real ELF. **Needs A12.** The pitch is unusually strong here: exact decimals, no float, no GC, no runtime, bounded memory and byte-identical IR is what embedded control code wants, and it is the one domain where "no floating point" reads as a feature |
| G5 | **An encoder to guard** — N1 / NOVELTY §1's serialization and database boundary exactness. `lib/json.bx` fired this trigger |
| G6 | N9 rows 6–9 + the *"money may not reach a model"* rule and its fixtures · borrow and mutability tracking for `dynamic` · M4 phases 4b–6 · static contract proving (SMT) · A4.6's deferred rows |
| G7 | **burxtQL** — a query language whose **contract IS its schema**, the same trick `burxt mcp-schema` already does one layer up. Specced nowhere; after N9 rows 6–9 |

---

## H — Forcing functions and the release gate

| # | Item |
|---|---|
| H1 | **A12's forcing function FIRED at v0.0.207**, and the promise was broken once. The ceiling went red in CI at **544 MB against 540**, while passing locally at **537** — the growth cumulative over v0.0.200–207, which added 143 lines to `emit.bx` alone with nothing re-measuring. Raised to **600 against the CI number**, because the 540 was set against a *local* 497 and CI runs ~7 MB higher, so the real margin was 3 MB rather than 43 — the exact mistake the comment above it warns about. **A ceiling must be set against CI, not the laptop.** The raise was taken because a red tree is the failure this project spent thirteen versions learning to avoid and slice 3 is not a hotfix. **CORRECTED v0.0.266, and this row was the roadmap's central argument for A12's priority: this ceiling was never A12's forcing function.** The growth was `self.globals`, a flat String appended once per string literal, quadratic in the compiler's own literal count — see **B29**. Chunking it took peak RSS **1,132 MB → 169 MB** and the rate **61.6 → 9.2 KB/line**, below every historical point on the trend including the 50.1 it started from. So the number that justified calling A12 *the last true blocker* was **one line of a data structure**, and every one of the three bar-raisings was paying interest on it. **A12 could not have fixed it even in principle**: the memory is 96% garbage, but the dead prefixes are interleaved in the arena with the live String still growing, so any release that reclaimed them would reclaim it too. A data-structure bug wearing a lifetime bug's clothes. **A12 keeps its other justifications** — a server loop building temporaries, the LSP re-checking on every keystroke, the freestanding target where the region must be a fixed buffer — and it loses this one. It should stop being argued for with this number |
| H2 | **Doc hygiene** — six spec headers still say `spec, to implement` for shipped work; `DESIGN.md` is stamped v0.0.152 and its *"Open tradeoff — Memory management"* was decided by M1; `spec/README.md` says *"as of v0.0.58"*; four audit rows are stale; **there is no effects spec in `spec/` at all**. **Fix each in whichever version touches it**, never as a separate cleanup — that is how they rotted |
| H3 | CI green **on the commit being tagged** — a tag on a red commit must be withdrawn, which happened with v0.0.171 |
| H4 | `cargo test --release the_release_tarball_works_without_rust_or_llvm -- --ignored` passes |
| H5 | **The 1.0 limitations document** — every `Decision` and every unpicked `Blocking` row, so nothing surprises anyone. This is what makes a high bar honest instead of optimistic |
| H6 | A stated **compatibility promise**, with `burxt review` as its mechanical enforcer |
| H7 | **DONE (v0.0.260) — four hosts, not one.** `release.yml` builds natively per architecture: `linux-x86_64`, `linux-arm64` (free on public repos since GA August 2025), `darwin-arm64`, `darwin-x86_64`. `fail-fast: false`, because one broken host must not hide whether the other three work, and `publish` refuses to attach anything unless **four** tarballs arrive — without that count a release would ship one platform and look complete |
| H8 | **DONE (v0.0.260) — a multi-arch OCI image**, `amd64` + `arm64`, from `scripts/Dockerfile`. It **copies** the binaries the matrix already built rather than compiling inside the image: a statically-linked LLVM 18 under QEMU is hours per architecture for a byte-identical result, and a build that slow stops being run. The image carries **gcc**, which is not a convenience — `burxt build` calls `cc` to link, so an image holding only the binary would pass `burxt check` and fail every build, reproducing the exact failure `install.sh` already warns about. It is **run** before it is pushed: build `linux/amd64`, execute `19.99 * 3`, refuse to push unless it prints `59.97` |
| H9 | **DONE (v0.0.260) — Windows, by container, deliberately.** Windows 11's `wslc` runs OCI images natively — no Docker Desktop, no third-party runtime (preview 29 June 2026, GA fall 2026). So H8's image *is* the Windows host, and the native MSVC port is refused with its bill written out in [ROADMAP-1.1](ROADMAP-1.1.md) §W2. The trigger that reopens it: someone who needs `burxt.exe` outside a container |
| H10 | **DONE (v0.0.260) — the release script stopped being Linux-only in a way that could not fail.** `ldd` does not exist on macOS, so the "does this binary link libLLVM?" guard found nothing there and **passed without looking** — for every Darwin build, silently. Now the tool is chosen per platform (`ldd` / `otool -L`) and an unknown platform is a hard stop. *A guard that cannot fail is not a guard.* Also `strip -o` → copy-then-strip, since GNU and BSD disagree about `-o` and a release runner is the wrong place to find out |
| H11 | **Distribution work that needs a machine we do not have → [ROADMAP-1.1](ROADMAP-1.1.md).** Android as a **host** (an experiment with the command written down, not a wall — NDK r27 *is* LLVM 18), the native Windows port, and the `use`-search-path question the container raised. The split is by **verifiability**: 1.0 holds what could be built and proven in one pass; 1.1 holds what cannot be finished by writing it |

---

## Not work — decisions on record

Changing these breaks the language. Each has its reason written down, and **all of them belong in H5.**

**Identity:** no floating point — upheld *and strengthened* by N9, where the flagship use nobody thought
was reachable without floats turned out reachable and better without them · no char type, no bare `s[i]`
· no reflection · no inheritance (dropped v0.0.46, *"composition-only is final"*) · no null · no GC, no
refcounting, no runtime · no `unsafe` escape hatch · no truthiness · **no removing Rust** — stage-0 is
the trust anchor and the differential · no file-level privacy.

**Shape:** bitwise as seven named builtins, not operators · String order is BYTE order, never locale
collation · no operator overloading · no C-style ternary · no `%` for modulo — it is the percent literal
· no format-spec mini-language in interpolation · no implicit prelude, glob imports or conditional
compilation · no undefined `Map` iteration order, ever · no wildcard `_` match arm · no stripping
contracts in any build mode · no `unwrap`.

**Stated-and-accepted costs:** contracts always checked · region granularity is coarser than Rust
(*"the honest limit of this model, accepted deliberately"*) · scale ceiling 18 · always-sret ·
declaration-order layout · **no catch/recover** — a failure exits 70 and there is no handler.

---

## Verification — every version, without exception

```sh
RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test --release
python3 scripts/site-examples.py --check
python3 scripts/site-reference.py     # when the compiler's surface changed
python3 editors/vscode/pack.py        # when a keyword was added
gh run list --limit 2                 # CI was red for 13 versions unwatched
```

The `-D warnings` is not optional: CI passes it and a plain `cargo build` does not, which is the third
distinct way CI has gone red while every local run was green.

- **Both compilers, or it is not done.** Stage-1 parity in the same version. The three process rules say
  why a green stage-1 is not evidence on its own.
- **A fixture per behaviour and a fail fixture per refusal**, with the reason in the fixture's comment.
  Raise the fail ratchet to the **measured** value with no cushion — a cushion *is* the drift.
- **Two streams, an exit status, or a cross-target claim needs a runner invariant**, not a `tests/pass`
  fixture — that harness compares stdout and requires success. `print_error_writes_to_stderr`,
  `a_program_reports_its_status_to_the_shell` and `the_ir_is_the_same_for_every_target` are the patterns.
- **Every claim added to a spec is verified by running the compiler.** M13 is why.
- **A change to the language is not finished until the highlighter, the language server and the packaged
  extension have changed with it** (M10 §2e).

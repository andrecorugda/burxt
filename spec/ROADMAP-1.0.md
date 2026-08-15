# Burxt 1.0.0 — the road to a language people can ship on

> Status: **the plan of record.** Written 2026-07-31 at v0.0.205, from a full read of all 26 specs plus
> `DESIGN.md` and three systematic scans of the compiler and standard library against Rust, Python,
> PHP, Java and Go. Every claim here was verified by **running** the compiler, not by reading it.
>
> This supersedes `FAR-HORIZON-ROADMAP.md`'s §4 ranking for near-term work. That document remains the
> **audit** — the row-by-row comparison and the record of which absences are decisions. This one is the
> **order**.

## Where it stands — audited v0.0.288, row by row

**Audited against the tree, not against the rows.** Andre asked for a checklist that also shows
*unchecked things that are done* and *checked things that are not* — and the first kind was the
larger. **Thirteen rows were complete and still read as open**, which is the failure this project
keeps paying for: a stale NOT-DONE is worse than a stale DONE, because nobody re-tests what the list
calls broken. They work around it.

| § | done | open | what is actually left |
|---|---|---|---|
| **A** compiler leverage | **12 / 12** | — | complete. A10 closed as DECLINED, not built |
| **B** bugs | **44 / 53** | 9 | B13 (a measurement) + eight stage-0/stage-1 divergences, B40–B51 |
| **Q** askable facts | 0 / 5 | 5 | all new, added v0.0.288 |
| **C** the rest of the bar | **1 / 2** | 1 | **C2 — dependency management. The last substantial 1.0 row** |
| **D** library floor | **23 / 26** | 3 | D2c tuple helpers · D2e monotonic clock · D2f chunks/windows |
| **E** crypto | **6 / 6** | — | complete |
| **H** release gate | **8 / 12** | 4 | H2 doc hygiene · H3 CI-green-at-tag · H6 compatibility promise · H11/H12 → 1.1 |
| **G** post-1.0 | 0 / 9 | 9 | out of scope for 1.0 by definition |

**So 1.0 needs, in order: C2 · H2 · H6 · H3 at the tag.** Everything else on this page is either
done, post-1.0, or a divergence that produces no wrong answer.

Corrected in this audit, all verified by reading the tree rather than the row: **D1i D1k D1m D1o
D1p D1q D1r D1t D2b D2d** were built and unticked · **H5** (the limitations document exists) ·
**H1** (discharged when A12 shipped) · **H4** (measured passing, 8.3 s) · **D1s** closed as
OBSOLETE rather than open, because A6 shipped and `range(n)` would now be a second way to do one
thing. Earlier in the same sweep: **B2, B6, B14** described defects that no longer existed.

## The release plan — Andre's, 2026-08-15

**1.0.0 ships after C2, and the order to it changed on the same call.** The audit had left the
critical path as C2 → H2 → H6 → H3; two things were promoted in front of C2 because they are small
and because one of them should not appear in a 1.0 at all.

| # | before 1.0.0 | why it is not deferrable |
|---|---|---|
| 1 | **B51** — `file_list_directory` answered `[]` for a directory that is not there | §B1's exact shape: **the silent wrong answer this language exists to refuse**, in the standard library, where a new reader meets it first. And the signature had to change, which is the one thing a compatibility promise makes impossible afterwards |
| 2 | **B40 · B43 · B44 · B47 · B48** — stage-0 and stage-1 disagreeing about what COMPILES | `README.md` and `DESIGN.md` claim two independent implementations that must agree. Eight known disagreements about what a valid program is undercuts a headline claim — the same class as B5, a published guarantee that was not enforced. B15 was this shape and cost an afternoon |
| 3 | **C2** — dependency management | the goal sentence says *depend on other people's code*. It is the only remaining row that is a **permanent commitment**: a manifest format and a resolution rule cannot be changed after people use them |
| 4 | **H2 · H6**, then **H3** at the tag | H6 is what makes the number mean anything |

**After 1.0.0, and this is the part that decides where everything else lands:**

- **1.0.x** — fixes only. No new function, no new syntax. That is what a patch is.
- **1.1.x / 1.2.0** — the additive leftovers: **D2c** tuple helpers, **D2e** monotonic clock,
  **D2f** chunks/windows, and all of **§Q**. These add functions and commands, which is a MINOR
  bump and not a patch. Calling them `1.0.1` would break the very rule `burxt review` exists to
  enforce mechanically, on the first release that promises it.
- **2.0** — everything in [ROADMAP-1.1](ROADMAP-1.1.md): hosts, and the whole web half — a real
  listening server, `html.bx`, `cgi.bx`. **That file is another session's and is not renamed here;
  the relabel needs relaying rather than editing.**

**And the rule that reconciles `burxt review` with a milestone number, which H6 should state
outright:** `burxt review` sets the **minimum** bump — it can prove a change weakened a promise and
therefore needs a major, and it can prove nothing was weakened. It cannot know that a release is a
milestone. So a human may always go HIGHER than the tool says, never lower. 2.0 for the web work is
exactly that: additive by the tool's reckoning, a major by intent.

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
| ~~B1~~ | **FIXED v0.0.280 — and the row's SYMPTOM was wrong, which matters more than the tick.** It does not answer `""`. **It ends the process**, exit 70, *"cannot open file for reading"* — verified. So B1 was never a silent wrong answer; it was that **there was no way to read a file that might not be there**, which is a different bug with a different fix. `file_read_maybe -> Option<String>` closes it and more than the row asked: `None` for missing, unreadable **or a directory**, `Some("")` for a genuinely empty file, and none of them stop the program. Pinned by `tests/panic/file_read_of_a_missing_file.bx`, because a pass fixture cannot pin behaviour that terminates. **`src/burxt-compiler/lsp.bx` already knew** — it refuses to use the module loader because of exactly this. ~~**`file_read` of a missing file answers `""`** — indistinguishable from an empty file. The silent wrong answer the thesis exists to refuse, in the standard library. Its own comment says the fix needs `Option`, *"which the language does not have yet"* — **it does**~~ | S |
| B2 | **ALREADY FIXED — row was stale, verified v0.0.283.** `os_byte_as_string` no longer exists; `lib/os.bx` records where it stood and why it went. The `byte_as_string(n)` builtin (A13, v0.0.260) replaced it and refuses rather than substituting. **`os_byte_as_string` is lossy** — every byte ≥ 127 becomes `"?"`. The only int→character path in the library, and it silently destroys data | S |
| B3 | **FIXED v0.0.280**, and the pid was only half of it. `os_pid` closes the collision; **`mkdir` closes the symlink half** — pids are small, guessable and reused, so `/tmp/burxt-fs-list-1234` is still a name an attacker can wait on. `mkdir` fails if the name is taken, in one syscall with no window, where `file_exists` then `write_file` has exactly that window and `/tmp` is where somebody is standing in it. `file_temp_directory` retries `mkdir(candidate, 0700)`; 0700 means the contents need no defending. Both fixed constants are gone. ~~**Hardcoded temp paths** `/tmp/burxt-fs-list`, `/tmp/burxt-os-capture` — two processes clobber each other, and both are a symlink-attack surface on a shared machine~~ | S |
| B4 | **FIXED v0.0.280** — `hash_equals_constant_time` in `lib/hash.bx`, spelled out in full for the same reason `divide_floor` is: the behaviour is the point. ~~**No constant-time compare.** `==` on Strings is `strcmp`, which short-circuits and **leaks the answer through timing** — every token and HMAC comparison~~ | S |
| B5 | **FIXED v0.0.284, both compilers.** `read_file`, `argument` and `c_string_at` now check, and **`os_env` is covered by `c_string_at`** rather than by a rule of its own — it is a library function in `lib/os.bx` reaching `getenv` through that door, so one place, not two that could drift. **The claim was already shipped and already false**: `docs/limitations.md` told a reader *"a String is UTF-8 and that is checked at every entry point"*, and a file of `0xff 0xfe` came back as a 22-byte String with exit 0. A published guarantee is exactly the kind a reader stops verifying. **One loop, not one branch per width**: the leading byte sets how many continuations follow AND the exact range the next one may take, and that single range rejects all four traps — `0xE0` demanding `A0..BF` kills the overlong three-byte form, `0xED` demanding `80..9F` kills surrogates, `0xF0` demanding `90..BF` and `0xF4` demanding `80..8F` kill the overlong four-byte form and everything above U+10FFFF; `0xC0`/`0xC1` never lead. The leftover count after the loop catches a sequence the buffer cut short — the case a per-width shape forgets, because it checks `i + width <= n` once and never looks again. **The stated cost arrived exactly where the row said it would**: the mascot invariant reads a GIF, and now reads it as `[Int]` through `file_read_bytes`'s primitives. That is the honest version — the old one relied on a String tolerating bytes that are not text, which is the property the language now denies. Pinned by `tests/panic/read_file_refuses_bytes_that_are_not_text.bx`, which writes its own bad bytes rather than committing a binary next to it. **The UTF-8 invariant is declared and unenforced** at all four entry points (`read_file`, `argument`, `os_env`, `c_string_at`). **Decided: validate.** Consequence: binary through `read_file` breaks → needs `file_read_bytes` (A1) | M |
| B6 | **ALREADY FIXED — row was stale, verified v0.0.283 by RUNNING the compiler, which is the only reason it was caught.** A grep of `is_reserved_name` said all nine were still unreserved; the compiler refuses every one of them — *``bit_and`` is a name the language owns, so a program may not declare it*. The grep was reading the wrong list. **9 builtins are not in `is_reserved_name`** (`bit_*`, `shift_*`, `c_is_null`, `c_string_at`) → a user program can shadow them | S |
| B7 | **And one consequence to carry forward, because it is a new obstacle on a target this repository claims:** the guard calls **`getrlimit`, which WASI does not have.** Nothing breaks today — wasm32 emits objects and does not link — but a browser or freestanding build now needs a compile-time fallback to a fixed floor. It is small, and it is listed here rather than discovered later by whoever first tries to link wasm. **AMENDED v0.0.286 — the v0.0.285 guard had a hole, and the hole was methods.** It went into `gen_fn`; methods are emitted by `gen_method`, a separate function. So `function f()` was guarded and `function (self) f()` was not, **and the suite went green** — the recursion fixture is a free function and none of 617 fixtures recursed through a method. Found from outside the suite: stage-1's parser is recursive descent written as METHODS, so a 30,000-deep expression segfaulted stage-1 while stage-0 parsed it fine. One `bt` in gdb showed `parse_primary → parse_expr → parse_primary` with no guard between the frames — **C1 finding a bug that C1's own successor commit created, the day after landing.** `tests/panic/a_runaway_method_recursion_is_named.bx` is the fixture that did not exist. **FIXED v0.0.285, both compilers.** Exit 70 and a named error, where it was exit 139 and a bare SIGSEGV — the last failure in the language that had no name, which `DESIGN.md` calls non-negotiable. **A stack FLOOR, not a signal handler**: `main` asks `getrlimit(RLIMIT_STACK)` how much stack this process was actually given and records where it runs out, less a 128 KB margin so the message itself has room to print; every function compares its own frame against that. `getrlimit` rather than a constant because the answer is 8 MB by default, unlimited under one `ulimit` and small inside a container — a guess is wrong in both directions. A probe `alloca` rather than `llvm.frameaddress`: an alloca in the entry block IS a stack address, needs no intrinsic, and spells the same in stage-1, which emits IR as text. **In every function, not only the ones a call graph calls recursive** — a static call graph cannot see recursion through a `dynamic` call, and a guard with a hole is worse than none because the hole is where the interesting recursion gets written. Cost: **+6.9% object size** (1,181,968 → 1,264,024 bytes on the 11k-line stage-1 compiler); a 50,000-frame recursion still runs. **Verified at `-O0` as well as `-O2`, and that matters**: at `-O2` LLVM turns some recursions into loops, so the fault never happens and a test written only against a default build would pass on a compiler with no guard at all. Finding that took C1's flag, which is a use for it that has nothing to do with debugging. Pinned by `tests/panic/a_runaway_recursion_is_named.bx`. **Stack overflow is the only failure Burxt does not name** — **re-verified v0.0.247 and still real**: non-tail recursion gives **exit 139, `Segmentation fault`, core dumped**, no named error. **And it bites NARROWER than the row implies, which is worth knowing before someone budgets for it:** a TAIL-recursive runaway does not overflow at all — `return f(n + 1)` becomes a loop under `musttail`, so it runs forever instead of crashing. So the failure mode is a hang for tail calls and a raw signal for everything else, and only the second wants a named error | S–M |
| B8 | **FIXED v0.0.283, both compilers.** Reproduced first, because the row said *is wrong* without saying how: written `[it != "make it so"]`, the failure read `requires tag != "make tag so"` — the `it` INSIDE THE STRING LITERAL was replaced along with the real ones, so the message quoted a string the program does not contain. Nothing computes wrongly, which is what makes it easy to leave: it is only the one artefact a failure hands a reader, misquoting the source it came from, in a language whose whole case is that a reviewer can see what happened. `replace_whole_word` now skips string literals in both `parser.rs` and `emit.bx`, with a backslash escaping the next byte and an interpolation counting as code again. Pinned by `tests/panic/a_bracket_clause_quotes_its_own_string.bx` — a **panic** fixture, because the message only exists when the contract fails and the pass harness requires success. Found on the way: **a bare `it` inside a string INTERPOLATION does not resolve at all** (*unknown variable: it*) — a separate gap, written down rather than folded in. A bare **`it` inside a string literal** in a bracket clause is wrong | S |
| B9 | **FIXED v0.0.283**, and it found a silent wrong answer next door. `\b`, `\f` and `\uXXXX` now decode, **including SURROGATE PAIRS** — JSON is specified over UTF-16, so every codepoint above U+FFFF is written as two escapes and an emoji is never one `\u`; a decoder handling each escape alone produces two half-characters that `from_codepoint` refuses outright as CESU-8. So pairs were never optional. A lone surrogate is an error rather than U+FFFD, for the reason `os_byte_as_string`'s `"?"` was wrong. **And the defect nobody was looking for: an escape that is not a JSON escape used to be SILENTLY DROPPED** — no arm matched, so the backslash and the character after it were both skipped and nothing appended, so `"p\qr"` parsed as `"pr"` with no error. One character quietly gone, inside the library that reads other people's data. Pinned by `tests/pass/json_string_escapes.bx`, which asserts the ROUND TRIP across all four UTF-8 widths. **`lib/json.bx` rejects valid JSON** (`\b`, `\f`, `\uXXXX`). Refuses rather than corrupts, but Burxt cannot read real-world JSON. Needs A5 | S–M |
| B10 | **MEASURED v0.0.286, and the dangerous half is closed.** The row's worry was a crash; B7's stack guard made it a NAMED refusal in both compilers, exit 70. What is left is a capacity difference, now with numbers instead of an estimate: **stage-0 parses 120,000 nested expressions without complaint; stage-1 refuses between 6,000 and 12,000** on an 8 MB stack, because stage-0 runs its walkers on a 512 MB stack and stage-1 gets whatever the OS gave the process. The row's *"~30k-node ceiling"* was wrong in both directions — too low for stage-0 by 4×, too high for stage-1 by 3×. **Kept open, downgraded to a capacity limit rather than a defect.** 6,000 levels of nesting is past anything hand-written and past most generated code, nothing computes a wrong answer, and the failure now says what it is. Iterative walkers are an M-sized rewrite of the recursive descent in both compilers, with real regression risk, for a case no real program reaches — so it waits behind work that fixes wrong answers. **Iterative AST walkers** — 512 MB stack, ~30k-node ceiling | M |
| B11 | ~~**M7: stage-1 compiles 101 of 102**; generic records in stage-1~~ **CLOSED, v0.0.215 — and it had been closed for a hundred versions without anyone writing it down.** 142 of 142; the one holdout needed `write_bytes`, which landed. Generic records emit in both compilers | — |
| B12 | ~~**stage-1 backend gaps**~~ — **there are none, measured v0.0.215: 142 of 142 pass programs, 0 refused.** The claim came from a stale `M4` §3b. **Re-pointed:** the real gap is the TOOLCHAIN — stage-1 has no LSP, no `burxt review`, no `mcp-schema`, so every non-compiler tool lives only in Rust. Moving one is a genuine capability claim | L |
| B13 | **CLOSED v0.0.297 by MEASURING it, which is what the row asked for and nobody had done for 177 versions.** Stage-1 compiles its own 11k-line source in **0.15 s on three consecutive runs**, against a budget of **20 s**. The 1.67× cannot be attributed retroactively — the v0.0.119 baseline is gone — and the question stopped mattering: a 130× cushion is not an instrument, it would not notice a hundredfold regression. **The budget is tightened to 5 s**, which is the *ratchet tightening* half of the row, still leaves room for a slow shared CI runner, and would catch one. M11's **1.67× compile-time growth is unattributed**; the ratchet tightening is pending | S |
| B15 | **FIXED v0.0.283 — by making stage-0 REFUSE it, not by teaching stage-1 to allow it.** The rest of the language decided that: a class body refuses a stray `;` and so does an enum, so an interface was the only declaration body that took one, which makes it an accident rather than a design. The line removed carried the comment *"a separating semicolon is allowed but not required"* — an optional separator is a second spelling of one thing, which is what closures were declined for. Both compilers now also say the same words, stage-1's, because they said it differently and stage-1's explains why: *expected `function` — an interface holds signatures*. `tests/fail/interface_signature_semicolon.bx` is the fixture nobody had written, which is the whole reason the differential could not see this for seventy versions. **stage-0 accepts a trailing `;` on an interface method signature and stage-1 refuses it.** Found by writing a fixture in v0.0.209 — no existing fixture used the `;` form, so the differential could not see it. A divergence in what is ACCEPTED, which is the direction that matters | S |
| ~~B16~~ | ~~**stage-1 has no C-boundary rule at all, so it ACCEPTS `function f(n: CInt) -> Int` that stage-0 refuses.**~~ **CLOSED v0.0.261.** Found by a13-bytes while scoping A7, verified from the commit rather than the tree: `check.bx` had **0** occurrences of the boundary message against `typeck.rs`'s **3**. `tests/fail/cint_in_burxt.bx` covered only the `let` form, which stage-1 refused *accidentally* through "declared CInt, but the value is Int" — the second process rule in the negative direction, since a fail fixture checks refusal and not the reason. The parameter form never had a fixture, so nothing could see it. Same shape and direction as B15: **a divergence in what is ACCEPTED.** `tests/fail/cint_as_a_parameter.bx` is the fixture that never existed; both compilers now refuse it with byte-identical text | — |
| B17 | **FIXED v0.0.287.** Stage-0's caret now lands on `CInt` — `1:20` with `^^^^` under the four characters — where it used to sit at `1:1`, on the `function` keyword. Moved to stage-1's choice, as the row said: an improvement, not a regression. **The cause was that the position did not exist where the error was raised**, which is C1's shape one layer up: `validate_type` answers a `String` and the caller attached the nearest span it had, the whole declaration. So `Param` now carries `ty_span`, set at the three places the parser builds one, and the seven `validate_type` call sites point at it. Fields get it too — `class R \{ n: CInt \}` now underlines the type. **And the reason it hid for twenty-five versions is that nothing looked**: a `.stderr` fixture records one compiler's text and `the_two_compilers_render_a_problem_identically` compares the rendered message; neither asks WHERE. `both_compilers_blame_the_same_token_for_a_boundary_type` asks, and asks in each compiler's own form — stage-0's caret, stage-1's `(at \`CInt\`)` — because requiring identical bytes would be requiring they render diagnostics the same way, which they do not and need not. What must match is which token they blame. **The two compilers agree on the boundary refusal's TEXT and disagree on its SPAN.** Measured at v0.0.261 on `function scaled(n: CInt)`: byte-identical sentence, but stage-0 points at **1:1** and stage-1 at **1:20**; the width form diverges the same way. Neither is wrong-as-such and **stage-1's is the better one** — it puts the caret on the offending type rather than the start of the declaration — so the fix is to move stage-0 to stage-1's span, an improvement rather than a regression. It hides because a `.stderr` fixture records one compiler's output and `the_two_compilers_render_a_problem_identically` compares the rendered message. **The span is not cosmetic**: it is where the editor draws the squiggle and what the LSP returns | S |
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
| **B40** | **FIXED v0.0.289.** Stage-1 refuses the cycle now, and the fix needed a **SECOND pass** — which is the part worth keeping. The declaration table is filled by the same loop that was doing the checking, so looking a class up during it answers *no such class* for every one of them, and the check silently did nothing. The direct self-containment check never noticed because it compares source TEXT rather than looking anything up. **A depth bound rather than stage-0's trail**, because a method may not take a `mutable` parameter, so a growing trail cannot be threaded through a recursive method — and the bound is not a heuristic: a simple path from a class back to itself visits at most as many classes as exist. Controls checked: nested non-cyclic classes and `lib/json.bx`'s recursion through a slice both still compile. **stage-1 accepts a mutual containment cycle that stage-0 refuses, and emits 24,780 bytes of IR for it.** `class A { b: B }  class B { a: A }` — stage-0: *"a `A` cannot contain a `A` — it would have no finite size"*; stage-1 compiles it. Stage-1's check catches only **direct** self-containment, not a cycle through a second class. Measured on v0.0.271: `check` exits 0 and the emit path produces real IR for a type that cannot have a size. The agent that found it saw stage-1 **spin until killed** on its variant, so the failure mode may be a hang or a nonsense layout depending on the shape — either way stage-1 is the permissive side on a program that cannot exist | S |
| **B41** | **FIXED v0.0.274.** ~~**The two compilers' "cannot return an array yet" messages differ in the last clause.** `function same(a: [Int; 3]) -> [Int; 3]` — stage-0: *"Return a class, or fill an array the caller owns."*; stage-1: *"Return a class **holding it**, or fill an array the caller owns."* Both refuse, so the differential's verdict check is silent; it is the text that differs. B17 family~~ | XS |
| **B42** | **FIXED v0.0.272 in stage-0** by the `Named` arm; stage-1 already refused it. ~~**B39's predicate hole is not only a trap — it is a LIVE use-after-free in stage-0 today, through B27's taint rule.** A `match` binding whose payload type is a generic INSTANTIATION is never tainted, because `may_be_region_storage` resolves a `Named` type through `structs`/`enums` while instantiations live in `made_records`/`made_enums`, so it answers "no" for `Wrapper$Int` and every other one. Proven, accepted by stage-0 and run: `enum Holder<T> { Full(T) }` carrying a `Wrapper<Int>` whose declaration has a `note: String`, matched inside a region and assigned out — prints `secret-value`, then the clobbering region's bytes **twice**. **Stage-1 REFUSES it**, because the agent porting the predicate fixed the Generic arm first — so stage-0 is the permissive side on a live use-after-free, which is the worst combination on this list. Predicted by the agent building A12 from the shape of B39 rather than found by a fixture: *"B27's taint rule is reading the same wrong answer"*. It was right, and the corpus had no program for it~~ | M |
| **B43** | **FIXED v0.0.289.** Stage-1's arm asked only about classes (kind 82) and an enum is kind 83, so `a == Shade.Light` fell straight through. **And stage-0's message was repaired in the same commit**: five diagnostics in `typeck.rs` carried runs of up to 26 SPACES, baked in when a `\`-continued string literal was reflowed and the continuation lost. Nothing had noticed because no fixture asserted that text. **`==` on an enum: stage-0 refuses it, stage-1 accepts it.** `enum Shade { Dark, Light }` then `a == Shade.Light` — stage-0 says *"`==` on the enum … is not available yet"*, stage-1 compiles it. **No region anywhere**, so it is nothing to do with the escape family — it was **unmasked** by B36's narrowing, which had been over-refusing region-shaped programs and hiding it. Stage-1 is the permissive side | S |
| **B44** | **FIXED v0.0.289.** Only a bare NAME on the left — `xs[0] = 9` is an element write and stays legal, which is what the message points at. **Whole-array assignment of a `[Int; 3]` field: stage-0 refuses, stage-1 accepts.** `kept = h.xs` — stage-0 says *"whole-array assignment is deferred"*, stage-1 compiles it. Also region-free, also unmasked by B36's narrowing. Stage-1 permissive | S |
| **B45** | **FIXED v0.0.274** — the index term is **dropped outright**, not gated. Gating was not enough: with a `[String]` element it fired again. ~~**stage-0's index arm keeps a disjunct that was reported as needing removal, and it is a false refusal.** `expr_allocates(index)` is still OR'd in — moved inside B36's gate rather than dropped — so `kept = xs[idx()]` with `xs` built OUTSIDE the region and only the *index* allocating is **refused by stage-0 and accepted by stage-1, and stage-1 is right**: nothing the index does can make the element region storage. Predates B36. **Deliberately NOT mirrored into stage-1**, because importing a false refusal to obtain agreement is the wrong direction — recorded in the arm's comment so the difference does not read as an oversight~~ | S |
| **B46** | **CLOSED v0.0.297.** Confirmed first: **0 of 365 `tests/fail/*.stderr` goldens contain a caret**, so a change collapsing every span to the start of the file would pass the whole suite — which is what B17 was, in one place, for twenty-five versions. **The fix is NOT a caret in 365 goldens**: that pins a column per fixture, so a message reflow becomes 365 edits and the suite gets re-recorded rather than read. `every_rejection_points_somewhere_and_not_all_at_column_one` asks the two questions that matter — every refusal carries a `-->`, and **they are not all column 1**. The second is the anti-vacuity half and the one that catches the regression: a collapsed span still produces a position and still renders. **The suite cannot see a span regression at all.** Not one `tests/fail/*.stderr` golden contains a caret line — they hold message text only, so a caret can move anywhere and every fixture still passes. `every_rejection_reports_a_position_that_points_at_code` checks that a position points *at code*, never how far it extends. **That is why B17 drifted for months**, and the only thing that caught it is the audit corpus's span column, which is not in CI. Same shape as the second process rule: a fixture set cannot tell "correct" from "nobody wrote the case" | S |
| **B47** | **FIXED v0.0.289.** And a near miss worth recording: the free-function arm ALSO refuses returning an interface object, and copying it here would have closed one divergence by opening another — **stage-0 accepts a method returning `dynamic Greet` and refuses a free function that does.** Measured before adding it, so it was not added. That asymmetry is now §B53. **A method returning a fixed array: stage-0 refuses, stage-1 accepts silently.** `function (self: Box) same(a: [Int; 3]) -> [Int; 3]` — B37's shape in the method spelling, which B37's fix did not cover. Stage-1 permissive | S |
| **B48** | **FIXED v0.0.289**, and it ran the direction the row predicted: stage-1 adopted stage-0's wording. It is the better message — `complain_at` already appends `` (at `nofunc`) ``, so the bare form named the function only in a suffix and the sentence could not stand alone in a log. **`unknown function: nofunc` against `unknown function`** — stage-0 names the function and stage-1 does not, so this divergence moves the OTHER way: stage-1 adopts stage-0's. Worth noting because every other text row this week ran stage-0 → stage-1 | XS |
| **B49** | **FIXED v0.0.297, both compilers.** `read_file` of a directory said *"region memory exhausted"* — naming the arena and blaming the reader's memory, when what happened is that `fseek` to a directory's end answers 9223372036854775807 and the allocation asked for eight exabytes. Now: *"cannot read this as a file — it is a directory, or it is larger than this build can hold"*, checked BEFORE the allocation, because afterwards the honest answer has already been replaced by the arena's. The bound is the region's own size, which makes one sentence true of both cases. **`read_file` of a DIRECTORY reports `region memory exhausted`.** Verified: `fopen` succeeds on a directory, `fseek` to the end answers **9223372036854775807**, and the reader tries to allocate it. So a directory handed to a file reader **blames memory**, naming the one thing that is not wrong. A user who hits this goes looking for a leak. Runtime source, and the fix is to reject the size before allocating rather than to enlarge the arena | S |
| **B50** | **FIXED v0.0.297 — and the NAME was never the problem, which is why the obvious fix was wrong.** The first attempt added twelve C symbols to the reserved list and would have broken the standard library: **`lib/files.bx` declares `fseek` itself**, as `whence: i32` — the real C type — and it works. The program that failed said `whence: Int`, which is i64 and simply false about C. So only a **DISAGREEMENT** is refused, and the check therefore cannot fall behind a list: a symbol codegen starts emitting tomorrow is covered the day it is added. Reported before the verifier runs, so LLVM's *"Call parameter type does not match function signature"* never reaches a user — a backend's diagnostic is the same defect as none, because it describes a call the programmer did not write. Pinned by an INVARIANT rather than a fail fixture: the conflict is only knowable at codegen, where there is no span, and stage-1 emits its own declaration as text and never reuses the user's. **Redeclaring a libc function the compiler also emits produces an LLVM VERIFIER error, not a Burxt diagnostic.** `external function fseek(f: CPointer, off: Int, whence: Int)` in a program that also uses `read_file` — for which the compiler emits its own `fseek` with an `i32` whence — gives *"LLVM module verification failed: Call parameter type does not match function signature! i32 2"*. **The message names nothing the user wrote**, and this is the one place a Burxt program can produce a diagnostic from a layer the language claims to hide. Found because §A7's widths were load-bearing for the first time: `whence` must be declared `i32`, and an `Int` silently redeclares `@fseek` so **every `read_file` in the program stops verifying**. Fix: refuse the conflicting redeclaration by name, or reconcile the signature | M |
| **B51** | **FIXED v0.0.288 — and the signature changed, which is the whole point of doing it now.** `file_list_directory` answers `Option<[String]>`: `None` for a directory that is not there, `Some([])` for one that is genuinely empty. The comment that stood in `lib/files.bx` DEFENDED the old behaviour — *"existing published API, and a caller can ask `file_is_directory` first"* — and it was wrong twice: empty and absent were the same answer, and asking first is a race, because the directory can go between the two calls. **What is checked is the exit status of `ls`, not the emptiness of its output** — a missing directory and an empty one produce the same empty text and different exit codes, and only one of those two facts can tell them apart. **Adding an honest twin beside the wrong one was rejected**: it means carrying the wrong one forever and making every reader ask which of the two they are looking at. There were two callers in the whole repository. This is the last version before a compatibility promise, so it is the last moment the signature could move at all. **`file_list_directory` answers `[]` for a directory that does not exist** — B1's exact shape, surviving in the one function that predates `Option` and has published callers. Empty and absent are different answers and the caller cannot tell them apart. Named in the module and the README rather than left silent; the fix is an `Option` return and a deprecation of the old spelling | S |
| **B52** | **FIXED v0.0.280**, with `tests/pass/a_one_field_class_in_an_array.bx` — the fixture that did not exist, which is why the suite read N of N over this for its whole life. ~~**stage-1 MISCOMPILES a one-field class held in an array — it reads back the address instead of the field.** Four lines, and about the most ordinary declaration anyone would write: `class R { n: Int }` then `let mutable rows: [R] = [R { n: 7 }]; print(rows[0].n);` — **stage-0 prints `7`, stage-1 prints a pointer.** `burxt check` says no errors on both, and stage-1's IR assembles and links. Silent wrong answer. The boundary is exactly one field: `{ s: String }` also fails (pointer, then garbage bytes), `{ fields: [String] }` **segfaults** when bound to a local, and **any second field makes both compilers agree** — as does the same class outside an array. **Pre-existing**, verified by rebuilding stage-1 from `git show HEAD:src/burxt-compiler/*.bx` in a clean directory. **Nothing in `tests/pass` puts a one-field class in an array**, which is why `the_burxt_backend_compiles_a_growing_share_of_the_suite` has read N of N over a real hole for its whole life — the same shape as B18, B35 and B42 before it: the suite cannot see what nobody wrote. Found by an agent writing `lib/csv.bx`, from a `CsvRow` that happened to have one field~~ | M |
| **B53** | **RULED and CLOSED v0.0.297 — both compilers now refuse.** A free function returning `dynamic Trait` was refused and a method was not: same spelling, same stated reason, two answers. **It needed a ruling rather than a fix, and the measurement is why:** the method version RUNS correctly, including across a call that reuses the stack, because an aggregate parameter is `byval` — the copy lives in the CALLER's frame and outlives the call. So stage-0 may be over-strict on free functions rather than unsound on methods, and **one experiment is not a memory model**. Between relaxing a safety refusal on a single run and making both refuse, the region model states its own direction: the failure is memory, never a dangling pointer. **Nothing in this repository returns a `dynamic` from anywhere**, so agreeing costs nothing today and would cost a compatibility promise once 1.0 ships. **stage-0 refuses a FREE FUNCTION returning an interface object and ACCEPTS a METHOD returning one** — the same spelling, the same stated reason. `function lend(b: Box) -> dynamic Greet` is refused with *"it borrows the value it refers to, which would not outlive the call"*; `function (self: Box) lend() -> dynamic Greet` compiles, runs, and answers correctly, **including across an intervening call that reuses the stack**. Found v0.0.289 while closing B47, by checking before copying the free-function refusal into stage-1 — copying it would have closed one divergence by opening another. **Not decided here.** The likely explanation is that an aggregate parameter is `byval`, so the copy lives in the CALLER's frame and outlives the call, which would make stage-0 over-strict on free functions rather than unsound on methods. That is a ruling about the memory model and does not belong to a change whose only job was making two compilers agree | S |
| **B54** | **FIXED v0.0.296, and the root cause is much wider than the row.** **Stage-1's emitter binds type parameters BY NAME**, so a generic whose parameter is `T` calling another generic whose parameter is also `T` let the caller's binding decide the callee's. Emitting `split_at$String`, the call `math_clamp(at, 0, len(xs))` — three Ints — came out as **`math_clamp$String`**, and `<` on String dereferenced two integers. Stage-0 emits `$Int`. **`T` is what everyone names it**, so this was a landmine under any library where one generic helper calls another; it needed no nesting and no unusual code. The symptom was arbitrary — here a segfault, elsewhere a quiet wrong answer. **The rule, arrived at by breaking two fixtures first: a CONCRETE argument decides the callee's parameter, an ABSTRACT one inherits.** `smaller(at, len(xs))` passes Ints, so the callee's `T` is Int whatever the caller's is. `echo<T>(x: T)` calling `identity(x)` passes the caller's own `T`, and that binding must carry through — `tests/pass/generics_functions.bx` says exactly that in a comment. An unconditional fix broke it, which is how the rule got stated rather than guessed. **And "concrete" had to mean concrete ALL THE WAY DOWN.** `set_new<T>() -> Set<T>` has no parameters at all, so only the return type decides it, and `Set<T>` is kind 46 whether or not `T` is settled — checking the top-level kind pushed an abstract binding that shadowed the caller's real one. `type_is_concrete` recurses. Pinned by `tests/pass/two_generics_sharing_a_type_parameter_name.bx`. **`a_tuple_of_slices_in_a_generic.bx` covered this exact shape for a hundred versions using `[Int]`, which passes either way** — the case was tested, tested for ONE element type, and that empty cell is precisely what §Q3 exists to print. `array_split_at` is back in `lib/array.bx`. **stage-1 SEGFAULTS on a tuple of two `[String]` slices returned from a generic — but only from inside `lib/array.bx`.** Found v0.0.294 while writing D2c. `array_split_at<T>(xs, at) -> ([T], [T])` works in stage-0, works in stage-1 for `[Int]`, and works in stage-1 for `[String]` **when the same function is copied into a standalone program** — so the trigger needs that file's other instantiations present. Exit 139, no message, stdout empty where stage-0 prints five lines. **`tests/pass/a_tuple_of_slices_in_a_generic.bx` already covers this exact shape and uses `[Int]`**, which is the whole lesson: the case was covered, covered for ONE element type, and the defect sat behind a green suite — B52, B18, B35 and B42 all over again, and precisely the empty cell §Q3's construct coverage would print. The function was written, tested, and TAKEN BACK OUT rather than shipped with a note: a library function that crashes under one of two compilers is not a library function. It returns when this closes | M |
| B14 | **ALREADY FIXED — row was stale, all four claims checked v0.0.283.** `lib/README.md` documents `Option` and `Result` at length; its module table lists **22 of 22** modules with none missing and none gone; `docs/reference/builtins.md` carries all nine bitwise and C builtins; and `map.bx`'s *"Burxt has no bitwise operators"* is simply TRUE — they are seven named builtins, which is the recorded decision, not rot. **Doc rot** — `lib/README.md` claims `Option`/`Result` do not exist · `map.bx` claims no bit ops · the module table omits 3 modules · `docs/reference/builtins.md` omits 9 builtins while claiming to be generated from that list | XS |

---

## Q — Questions the compiler can already answer, and nothing can ask

**Why the letter is out of order.** It sits here, between the bugs and the rest of the 1.0 bar,
because its position says its priority and its letter would have meant reletter­ing `C1` and `C2`
in every place they are cited. `spec/A7.0-NAMING.md` §9 is the precedent: a derived name is a
reference no sweep can see.

**What unifies these rows, and it is not "tooling".** Each is a fact the compiler *already
computes and already enforces*, that a person cannot get at. Effects are checked on every call and
there is no way to ask what a program reaches. Every construct in the language is enumerable and
there is no way to ask which ones the suite has never produced. Allocation is analysed per function
and there is no single sentence that says what a function will not do. Guarantees are written in
prose and nothing ties one to the fixture that proves it.

That last shape is why this section exists rather than being scattered through §F. **The thesis is
that a reviewer can see that an agent did not make a costly mistake.** A compiler that knows the
answer and cannot be asked is failing the second half of the sentence, and every row below is the
same failure wearing different clothes.

| # | Item | Size |
|---|---|---|
| Q1 | **`burxt effects <file.bx>`** — what can this program reach? Report the effect set with WHERE each one entered, and `--allow <list>` exiting non-zero on anything outside it. The vocabulary is closed and already enforced — `clock`, `commands`, `files`, `input`, `model`, `network` — and a function that reaches one must say so (*"`os_capture` touches files, but `snoop` does not say it does"*). **The per-function analysis exists; nothing aggregates it over a program.** This is the natural sibling of `burxt review`: that one answers *did this change promise less*, this one answers *what can this touch at all*, and together they are the review story for machine-written code. **Nothing else can answer it** — you cannot ask Python or Go whether a program touches the network, because nothing ever recorded it | **S** |
| Q2 | **DECISION, Andre's: the top level is exempt from effects.** Verified v0.0.287 — `use "lib/os.bx"; os_capture("id")` as a bare statement compiles clean, while the same call inside a function is refused until the signature says `touches files`. So every FUNCTION in a file is honestly labelled and the program's own entry point, which is what a reviewer reads first, is silently unlabelled. Two defensible answers: **(a)** require `program touches ...` on a file that reaches anything — consistent, and **it breaks every existing program that reads a file**; **(b)** leave the top level free and make Q1 the way to ask. Recorded as a decision and not as work, because (a) is a language change and that is not mine to make | — |
| Q3 | ⭐ **`burxt coverage --constructs`** — which shapes of program has the suite never once produced? **The single highest-value row in this section, and it is written from the evidence rather than from taste.** Nearly every real defect this project has found had **100% line coverage on the function that was wrong**: B52 (a one-field class in an array), B7's method hole, B15 (a trailing `;`), B17 (nobody checked WHERE a refusal pointed), and the escape family ~7 times over. The suite ran the code; it never ran it on that SHAPE. Each is an empty cell in a cross-product of construct × context — and the compiler knows both axes (41 typed-expression kinds, the type kinds, the contexts: in a region, behind `mutable`, under `dynamic`, inside a contract). Instrument the compiler while it builds the suite, record which pairs were ever produced, print the empty cells. **This is only possible because the vocabulary is small and CLOSED** — no closures, no reflection, no conditional compilation, one spelling per thing. In C++ or Rust the cross-product is unbounded and the idea is meaningless. It converts every feature this language refused from an apology into a capability | M |
| Q4 | **A certificate of what a function will NOT do** — one statement combining `allocates nothing`, the effect set, the contracts, and `decreases`: *this function allocates nothing, touches nothing, terminates, and can fail only in these named ways*. Every piece is built or half-built; nothing assembles them. Rust cannot say this — `no_std` is a build configuration, not a per-function proof, and there is no allocation guarantee in a signature. This is what hard real-time, medical, avionics and exchange code buys today with manual audit and MISRA | M |
| Q5 | **A guarantee that cannot go stale** — each claim sentence in `docs/limitations.md` and the guide carries the fixture that proves it, and the build fails when that fixture is missing or vacuous. **Four documented lies in one session** paid for this row: the page said UTF-8 was checked at every entry point (it was not — that became B5), said the debugger did not exist (it did), said the crypto was "being built" two versions after it shipped, and `cli.md` published a malformed row for months. Doctests prove an EXAMPLE runs; this proves a CLAIM is enforced, which is a different thing and the one this project keeps getting wrong. **A stale "we do not do that" is worse than a stale "done", because nobody re-tests what the list calls broken — they work around it** | S |

---

## C — The rest of the 1.0 bar

| # | Item | Size |
|---|---|---|
| C1 | **DONE v0.0.282 — and the correction below was right, the refactor was the job.** `burxt build -O0 -g` emits DWARF: a line table at statement granularity, a subprogram per function, a `DILocalVariable` per parameter and `let`, and lexical scopes so `info locals` shows what is actually in scope. **Verified in gdb, not inferred from flags**: a breakpoint resolves to the written line, `doubled = 42` and `label = "widened"` print as VALUES (a String is described as pointer-to-char, so a debugger shows text where it would otherwise show an address), the backtrace names the caller's line, and across a `use` boundary a function is attributed to **its own file** rather than to the flattened buffer. **The acceptance case that earned its place was the contract failure**: a debugger can break on the failing `requires` clause itself and read the arguments that violated it — `balance=100, amount=500`. Probing it found a real defect nothing in the suite covered: a contract runs in the function PROLOGUE, before any statement has set a position, so its instructions carried no location and a clause calling a `pure` function failed LLVM's verifier outright. **The fixpoint is safe by measurement rather than by argument: all 202 pass fixtures emit BYTE-IDENTICAL default IR before and after**, because debug info is opt-in — which is also why it must be, since `-g` records an absolute directory and a producer string. Cost, on the 11k-line stage-1 compiler: `-g` adds **+14% object size and +1.3 s**; the rest of the `-O0 -g` size (1.18 MB → 2.31 MB) is unoptimised code, and `-O0` compiles it **about 4× faster (~13–20 s → ~3.5 s)**, which is a win on its own. **That number is a correction**: v0.0.282 first recorded it as 59.4 s → 5.9 s, a 10× ratio, from a SINGLE timing taken while a test suite was running on the same machine. Re-measured five times on a quiet machine it is ~4×, and the `-O2` figure is noisy enough (12.7 s to 19.6 s across three runs) that no precise ratio is honest. One sample under load is not a measurement — the same mistake as quoting a stale binary, in a stopwatch. Stage-0 only, as the row planned. **CORRECTED v0.0.281 — this row under-describes the job by one refactor.** It says *"every node already carries a span, so the information exists and is being discarded."* That is true of `ast::Stmt`/`Expr` and **false of the typed AST codegen actually consumes**: `TypedExpr` is `{ ty, kind }`, `TypedStmt`'s thirty-odd variants carry **zero** spans, and neither does `TypedFn`. Verified by grep. The only span surviving the checker is `expr_types`, an LSP-hover side table. **So the information does not exist where DWARF is built — it is discarded one layer earlier than this row assumes**, and the three cheap ways around it are all unsound: a parallel walk of the untyped tree dies because `place_releases` (`typeck.rs:10706`) rebuilds the typed body and **inserts `Release` nodes with no untyped counterpart**, so the trees are not 1:1; a pre-order side table dies the same way; and counting-based correlation is right about every case someone wrote and silently off-by-one on the case nobody did — **and its symptom is a debugger reporting the wrong line, which is worse than no debug info at all.** The honest fix is a span on the typed node: `TypedStmt { kind: TypedStmtKind, span }`, wrapped at the single funnel, with `place_releases` carrying it through. **Add the refactor to the estimate.** **DWARF debug info + an `-O0` flag.** Stage-0 only for 1.0, stated as such. Matters because *an agent that cannot debug inserts `print`, which moves the stack and changes the answer* — the v0.0.141 trap | M |
| C2 | **DONE v0.0.293 — slice 4 of 4: `burxt review --semver`, the mechanical semver rule.** **It answers a DIFFERENT question from the default mode**, which is why it is a flag and not a replacement: the default asks *did this promise less* — a reviewer of an agent's diff — and `--semver` asks *can a consumer upgrade without editing their code*. They disagree in two places, both counter-intuitive. **A stricter `requires` is a MAJOR**: it promises MORE, the default mode correctly reports nothing weakened, and every caller that satisfied the old signature may now fail. That is the flagship catch run backwards — deleting a precondition is the agent mistake `review` exists to find, and adding one is a breaking change. **A weaker `ensures` is a major** even though nothing at the call site changed. **And the rule nothing else has to think about: a public function that gains an EFFECT is a major**, because effects propagate — every caller must write `touches files` in its own signature or stop compiling. In a language where effects are not in the type, that change is invisible and ships as a patch. **`public` is what makes any of it possible.** A change to a declaration no consumer can name is a patch; before slice 2 every helper was indistinguishable from the interface and the only honest answer was *major, always*, which is the same as no answer. **The limit is printed in the output rather than filed in a footnote**: it reads the INTERFACE, not the behaviour, so it can prove *at least a major* and prove *nothing in the interface broke*, and can never prove *safe to upgrade*. **A floor, never a ceiling — a person may always go higher, never lower**, which is also what reconciles it with a 2.0 chosen for a milestone. `--require patch|minor|major` exits 1 when the bump claimed is smaller than the one demanded, so it is a CI gate without parsing output; it is deliberately not a build gate, because a compiler that refuses to build over a version STRING is enforcing policy rather than correctness. **Slice 3 of 4 landed v0.0.292: the lockfile and `burxt fetch`.** **A build does not touch the network** — that is the design decision, not an unfinished edge: a build that fetched silently would do different things on different days depending on what a remote had done, which is the opposite of every other guarantee here. `burxt fetch` is the one place, and only when asked; `build` reads what is on disk and refuses by NAME, *"needs the dependency `greeter`, and it has not been fetched. Run `burxt fetch`"*, rather than reporting a missing file the reader never meant to create. **With a lock present the LOCKED COMMIT is checked out, not the tag** — a tag is a name somebody else can move, and the second person to fetch a project should get the bytes the first person built. **Proved by moving one**: `a_lockfile_pins_a_commit_even_when_the_tag_moves` publishes v1.0.0 answering 42, locks it, rewrites the upstream so the same tag answers 1041, fetches again, and asserts the build still says 42. Reading the lock back would have proved only that the writer ran. A locked commit that has vanished gets its own message, because *the history was rewritten* and *no such tag* need different advice. **Stage-0 only, and the counterpart map says why**: fetch moves files and touches the network, and neither changes what a program MEANS — stage-1 finds a fetched package by the same derived cache path without needing to know a lock exists, so there is no divergence to have. **Slice 2 of 4 landed v0.0.291: `public` at the package boundary, both compilers.** Andre's ruling, and the keyword is spelled out because every other name in this language is — `pub` would have been the single abbreviation in the vocabulary. **Everything is visible inside a package; only `public` declarations are reachable from a package that depends on it**, so no existing program changed. **Privacy is a RELATION, not a property, and learning that cost the first implementation.** The obvious design — drop non-`public` declarations from the program — hid a dependency's helper from *its own package*, so `tax_of` could not call `rounded`. The check has to be at the point of USE, comparing the package of the use with the package of the declaration. **And placement mattered twice more.** In stage-1 the check first went beside `find_function`, where several branches return earlier, so it fired for some calls and not others; it belongs at the top of the call branch. `declared_type` cannot refuse at all — immutable `self`, nothing to report with — so the `let` annotation is checked where it is read. Four cases pinned in both compilers by `a_package_dependency_resolves_and_an_ambiguous_import_is_refused`: a public function reachable, a private one refused **with the caret on the reaching file rather than the dependency**, a public class reachable, a private class refused. Grammar, web highlighter, packaged `.vsix` and the reference all updated in the same version, per `M10 §2e`. **IN PROGRESS. Slice 1 of 4 landed v0.0.290: the manifest and package resolution, in BOTH compilers.** `burxt.package` — one statement per line, first word is the key, no nesting, no quoting, no expressions. **Not TOML** (no parser exists, and adding a grammar to read three keys is a dependency for the sake of punctuation) and **not a Burxt program**, which was the tempting answer for a self-hosting language: *a manifest that can compute is a manifest a reviewer has to execute in their head*, and Gradle is what happens when the build file becomes a language. A `use "money/tax.bx"` whose first segment names a declared dependency resolves under it; **everything else stays exactly what it was**, a path relative to the importing file, so no `use` in this repository moved. **An import readable BOTH ways is refused rather than resolved** — picking silently would make resolution depend on the shape of a directory tree, so the program would compile here and fail on somebody else's machine, which is the failure a lockfile exists to prevent and would not catch. **No version ranges, ever**: a dependency names one tag and the lockfile will pin one commit, so resolution is a lookup rather than a solver. Ranges are what make dependency resolution a research problem, and the thing they buy — automatic minor upgrades — is exactly what `burxt review` is meant to make a decision rather than a default. Both compilers give the same answer and the same refusals, word for word, pinned by `a_package_dependency_resolves_and_an_ambiguous_import_is_refused`. **Remaining: the lockfile, the git fetch into `.burxt/packages/`, `public` at the package boundary, and `burxt review` as the semver rule.** **Dependency management** — manifest (git URL + tag), lockfile, local cache, `pub`/visibility. **No registry for 1.0.** `burxt review` becomes the semver rule: a major bump is *mechanically detectable*, which nothing else can do | L |

---

## D — The standard-library floor: full Rust `str` + `Vec` parity

**D0 — DECIDED v0.0.279, and measured rather than chosen: a CHUNK LIST joined PAIRWISE.**

Accumulate into a `[String]`, flush the pending piece when it passes ~128 bytes, and join by repeated
**pairwise** merge. Never `out = out + piece` in a loop, and never a left fold at the end.

The numbers come from this week's compiler work rather than from theory, which is why this is a
decision and not a preference:

| | before | after |
|---|---|---|
| `self.globals`, one flat String appended per string literal (**B29**) | 1,132 MB | **169 MB**, 5.4× faster, byte-identical output |
| escaping one 8,000-byte literal to IR, quadratic in *that literal's* length (**B30**) | 100 MB | **9 MB**, flat where it had been quadratic |

Three things that generalise to every function below:

- **The join must be pairwise.** Folding left rebuilds the whole prefix at every step, which is the
  same quadratic you just escaped, one level up.
- **Measure the flush point; do not inherit it.** `write_body` went 512 → 128 for another 9 MB, while
  `write_module`'s threshold turned out **inert**, because its caller already fed it 512-byte pieces.
- **A String's `len` walks it**, so never leave `len(s)` in a loop condition — that alone made the
  lexer quadratic once.

`join_chunks` in `src/burxt-compiler/emit.bx` is the reference implementation. This project has now
paid for this four times (v0.0.68, v0.0.77, v0.0.82, and B29/B30 this week); the fourth time cost
963 MB in the compiler's own memory.

### D1 — writable today, no compiler change

| # | Module | Functions |
|---|---|---|
| ~~D1a~~ | **DONE v0.0.280.** Case (`to_upper_ascii`/`to_lower_ascii`/`capitalise`/`title_case`/`equals_ignore_case`, the `_ascii` in the name carrying the limit), `replace`, codepoint-aware `reverse`, pad and trim. ~~**`lib/string.bx`** transform~~ | `to_upper_ascii` · `to_lower_ascii` · `capitalise` · `title_case` · `equals_ignore_case` · `find_ignore_case` · `compare_ignore_case` · **`replace`** · `replace_first` · `reverse` *(needs A5)* · `pad_start` · `pad_end` · `pad_centre` · `trim_start` · `trim_end` · `trim_bytes` · `strip_prefix` · `strip_suffix` · `slice` *(end-exclusive)* · `insert` · `remove` · `shorten` · `squeeze_space` · `indent` · `dedent` |
| ~~D1b~~ | **DONE v0.0.280.** Search and split. ~~**`lib/string.bx`** search & split~~ | `rfind` · `find_from` · `count` · `find_any` · `rfind_any` · `contains_any` · `find_at -> Option<Int>` · `split_space` · `split_any` · `split_times` · `rsplit` · `split_once` · `split_inclusive` · `split_no_empty` |
| ~~D1c~~ | **DONE v0.0.280.** Classify. ~~**`lib/string.bx`** classify & compare~~ | `is_empty` · `is_blank` · `is_digit` · `is_alpha` · `is_alnum` · `is_upper` · `is_lower` · `is_punct` · `is_hex_digit` · `is_ascii` · `all_digits` · `all_alpha` · `compare` *(3-way)* · `compare_natural` · `common_prefix_len` · `edit_distance` |
| ~~D1d~~ | **DONE v0.0.280.** Parse and format. ~~**`lib/string.bx`** parse & format~~ | `parse_int_base` · `parse_hex` · `int_to_base` · `int_to_hex` · `int_to_binary` · `int_padded` · `int_grouped` · **`parse_decimal`** *(per scale)* · `decimal_padded` |
| ~~D1e~~ | **DONE v0.0.280.** `lib/array.bx` reshaping, plus the whole higher-order family. ~~**`lib/array.bx`**~~ | **`slice`** · `copy` · `concat` · `insert_at` · `remove_value` · `count_of` · `last_index_of` · `binary_search` · `equals` · `dedup` · `product_int` · `repeat` · `take` · `drop` · `pop` *(precondition form)* · `rotate` · **a faster stable sort** |
| ~~D1f~~ | **DONE v0.0.280.** `map_values`, `map_entries` (which needed **tuples**, landed the same day), and the rest. ~~**`lib/map.bx`**~~ | **`values()`** · **`entries()`** · `clear()` · `merge()` · `take()` · `is_empty()` · `map_increment` · `map_from` |
| ~~D1g~~ | ~~**`lib/set.bx`**~~ **DONE v0.0.251** — 367 lines, `class Set<T: Equatable>` over `Map<T, Bool>` with `add`, `add_all`, `has`, `remove`, `take`, `count`, `items`, `is_subset_of`, `equals`, `union`, `intersect`, `difference`. **`take() -> Option<T>` is the §D2a item, and it was unwritable this morning** — an Option-returning generic needed A3, which landed hours earlier. Reads under both compilers |
| ~~D1h~~ | ~~**`lib/math.bx`**~~ **DONE v0.0.249** — 528 lines, 24 declarations, `INT_MAX`/`INT_MIN` as folded consts, and all three `checked_*` are `pure`. **The overflow ORDER is the design:** Burxt's `+` traps, so `checked_add` cannot compute-then-test — it asks `math_add_overflows` first, which is why those three predicates exist as public functions rather than hiding inside. Measured: `checked_add(INT_MAX, 1)` answers None without crashing, `isqrt(15)` is 3 and `isqrt(16)` is 4 exactly. Reads under both compilers |
| D1i | **DONE — verified v0.0.288 by reading `lib/decimal.bx`, not by trusting the row.** `decimal2_abs`, `decimal2_is_zero`, `decimal2_sign`, `decimal2_percent_of`, `decimal2_round_to`, `decimal2_cents`/`from_cents`, the same family at scales 4/6/7, `divide_round_half_even`, and **`money_split`** — the largest-remainder penny allocation this row called the canonical exact-money problem. **Decimal helpers** | per-scale `abs` · `min` · `max` · `is_zero` · `percent_of` · `round_to` · **`money_split`** — largest-remainder penny allocation, *the* canonical exact-money problem, and absent |
| ~~D1j~~ | ~~**`lib/time.bx`**~~ **DONE v0.0.255** — 534 lines, `DateTime` and `Duration`, Hinnant's `days_from_civil`/`civil_from_days` (exact integer arithmetic, no tables), ISO-8601 format and parse, `weekday`, `day_of_year`, `is_leap_year`, `days_in_month`. **UTC only and it says so**, per `DESIGN.md`'s commitment that *"dates/timezones, when they come, arrive timezone-explicit or not at all."* Verified on the cases that catch a subtly-wrong date library: **1900 not leap, 2000 leap** (the pair that catches a wrong century rule), 1970-01-01 = day 0, 1969-12-31 = **-1** so pre-epoch works, `1700000000` → `2023-11-14T22:13:20Z` round-tripping, and **`2024-02-30` parses to None** rather than being accepted. Monotonic and sub-second still need A7, and that limit is named rather than approximated |
| D1k | **DONE — `lib/random.bx`.** `random_from(seed)`, `next`, `next_below`, `next_between`, `random_shuffle`, `random_choice`, `random_unsigned_at_most`. The naming decision held: seeded and reproducible on purpose, and the CSPRNG lives under a different name in `lib/secure.bx`. **`lib/random.bx`** *(new)* | seeded xorshift/PCG · `next_below` · `next_between` · `shuffle` · `choice`. **Named `random_from(seed)`, never a bare `random()`** — reproducible on purpose, wrong for keys |
| ~~D1l~~ | **DONE v0.0.280 — `lib/path.bx` is new.** POSIX-only and it says so. Verified on the cases that decide a path library rather than the happy path: `normalise("/..")` → `/`, `normalise("../..")` unchanged, `.hidden` has **no** extension, `join("a/", "/b")` does not double the separator, `dirname("a")` → `.`, `basename("/a/b/")` → `b`. ~~**`lib/path.bx`** *(new)*~~ | `join` · `basename` · `dirname` · `extension` · `stem` · `is_absolute` · `normalise`. **None exist at all** |
| D1m | **DONE — `lib/files.bx`.** `file_read_maybe`, `file_read_bytes`, `file_size`, `file_is_file`/`is_directory`, `file_copy`/`move`/`delete`, `file_make_directory`/`remove_directory`, `file_list_directory`, `file_walk`, `file_temp_directory`/`temp_path`/`temp_release`. **`lib/files.bx`** | `read_maybe -> Option<String>` · `is_directory` · `is_file` · `size` · `copy` · `remove_directory` · `walk` · `read_bytes` *(A1)* · `temp_file` |
| ~~D1n~~ | **DONE v0.0.280 — `lib/log.bx` is new.** stderr not stdout (verified: stdout carries only the program's output), ISO-8601 via `lib/time.bx` rather than a second date formatter, quiet by default, threshold case-insensitive. **An unknown `BURXT_LOG` opens the log rather than silencing it**, and `log_env_problem()` returns the complaint as an `Option<String>` for a caller to print — a library that prints unprompted is the wrong shape. ~~**`lib/log.bx`** *(new)*~~ | `debug`/`info`/`warn`/`error` · `BURXT_LOG` threshold · stderr · timestamps. Closes the audit's `structured logging: Blocking` |
| D1o | **DONE — `lib/result.bx`.** `assert_that`, `panic`, `todo`, `unreachable`, `result_context`, `option_ok_or`, `result_is_error`. All four stoppers exit 70 and say `burxt panic:` so a reader knows which layer noticed. **Errors** | `assert_that(held, why)` · `panic(why)` · `todo()` · `unreachable(why)` · `result_is_error` · `result_context` · `option_ok_or` |
| D1p | **DONE — the UTF-8 layer is in `lib/string.bx`.** `char_count`, `char_at`, `from_codepoint`, `codepoint_at`, `is_valid_utf8`, `to_bytes`/`from_bytes`, `string_chars`. And from v0.0.284 the invariant is ENFORCED at every entry point rather than merely declared — see B5. **UTF-8 layer** *(A5)* | `next_char` + `CharAt` · `char_count` · `char_at` · `char_index` · `from_codepoint` · `codepoint_at` · `is_valid_utf8` · `is_continuation` · `from_byte` *(retires the lossy `os_byte_as_string`)* · `to_bytes` · `from_bytes` |
| D1q | **DONE — `lib/os.bx`.** `os_capture_status` (stdout, stderr and exit code separately, no longer merged with `2>&1`), `os_pid`, `os_cwd`, `os_platform`, `os_set_env`. **Process / env** | `os_capture_status` — stdout, stderr and exit code separately; today they are merged with `2>&1` · `os_set_env` · `os_pid` · `os_cwd` · `os_platform` |
| D1r | **DONE — `os_sleep` in `lib/os.bx`**, over an `usleep` extern. **`sleep(ms)`** | A five-line extern. **Blocks every retry and poll loop today** |
| D1s | **OBSOLETE, not open — A6 shipped.** This row was explicitly *"a stopgap until A6"*, and `for i in 0..n` exists, so building `range(n) -> [Int]` now would add a second way to do one thing. Closed as superseded rather than ticked as built. **`range(n) -> [Int]`** | Stopgap until A6 |
| D1t | **DONE — `lib/csv.bx`**, RFC 4180 with every deviation named in the header, and parse/write/parse/write pinned in `tests/pass/csv_library.bx`. **`lib/csv.bx`** *(new)* | read + write. JSON is covered thoroughly; CSV is the other universal interchange format, and the one a money language is handed most |

### D2 — needs A first, listed so nothing is written twice

| # | Item | Needs |
|---|---|---|
| ~~D2a~~ | ~~`array_pop<T> -> Option<T>` · generic `Set` · `map.take` · `option_ok_or`~~ **A3 UNBLOCKED v0.0.241 and the payoff is cashed:** `array_pop<T>` measured working in both compilers, and `lib/set.bx`'s `take() -> Option<T>` shipped v0.0.251 — a generic Set with an Option-returning method, which is the whole row | ~~A3~~ done |
| D2b | **DONE.** `string_reverse` is codepoint-aware, and case handling names its limit (`string_to_upper_ascii`). JSON `\u` closed with B9 at v0.0.283, surrogate pairs included. Codepoint-correct `string_reverse`, case handling, char indexing, JSON `\u` | A5 |
| D2c | **DONE v0.0.294** — `array_zip` and `array_enumerate`, over tuples (A8). `array_split_at` was written, verified in stage-0, and TAKEN BACK OUT: it segfaults under stage-1 for a `[String]` element, from inside `lib/array.bx` and nowhere else — **§B54**, found by writing it. `divmod` and `char_indices` are not here: `divide_floor`/`remainder` already answer the first as two named calls a reader can see, and the second waits on B54's shape. `zip` · `enumerate` · `char_indices` · `split_at` · `divmod` | A8 |
| D2d | **DONE — `lib/fn.bx` + `lib/array.bx`, through GENERIC INTERFACES rather than closures (A9).** `Mapper`, `Predicate`, `Folder`, `Comparer`, then `array_map`, `array_filter`, `array_fold`, `array_any`, `array_all`, `array_sort_by`, `array_retain`, `array_partition`, `array_position`. This row is why A10 could be declined: `dynamic Trait` was already a function value and A9 made it generic. `map` · `filter` · `fold` · `any` · `all` · `sort_by` · `retain` · `partition` · `position` | A9 or A10 |
| D2e | **DONE v0.0.294 — `time_wall_micros` and `time_since_micros`, and the name says which clock it is.** **Monotonic is not reachable and that is a LANGUAGE decision, not an oversight**: `CLOCK_MONOTONIC` is **1 on Linux and 6 on macOS**, and Burxt has no conditional compilation — a recorded decision — so no single program can name both. `CLOCK_REALTIME` is **0 on both**, which is the only one reachable portably. The stated cost: it is a WALL clock and can step backwards when the machine's time is corrected, so a duration across an NTP step can be negative. Fine for timing a compile; wrong for a timeout that must never go backwards. Reading it needs eight bytes reassembled by hand, because the pointer wall hands over bytes and nothing else. Monotonic clock · sub-second time · benchmarking · timeouts | A7 |
| D2f | **CLOSED as BLOCKED, and the row's premise was wrong.** It files `chunks`/`windows` under *"needs a slicing decision"*. That decision was easy and it is COPY — a borrowed view outlives nothing safely without a lifetime to check it against, and this language deliberately has none. **The actual blocker is that `[[T]]` DOES NOT EXIST**: *"a growable array cannot hold another array yet — its element would need its own region"*, measured v0.0.294. Both functions answer a list of lists, so neither is writable until a growable array can hold one — a language feature, not a library choice. Recorded in `lib/array.bx` where someone reaching for `chunks` will find it. `chunks` · `windows` · borrowed sub-slices | a slicing decision |

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
| E1 | **DONE v0.0.280 — `lib/hash.bx`.** SHA-256, SHA-512, HMAC, PBKDF2, **verified against published vectors by running them**: `sha256("")` → `e3b0c442…b855`, `sha256("abc")` → `ba7816bf…15ad`, `sha512("abc")` → `ddaf35a1…a49f`, HMAC-SHA256(key, fox) → `f7bc83f4…3cd8`. Every one exact. Written in a language with **no unsigned 32-bit arithmetic** — A7's widths are boundary-only — so the masking and rotation are explicit and tested. ~~**SHA-256 / SHA-512 · HMAC · PBKDF2**~~ | **BUILD** — published test vectors, no secret-dependent branching. Verifiable exactly as CRC-32 already is, so "it compiles" and "it is correct" become one statement |
| E2 | **DONE v0.0.280 — `lib/encoding.bx`.** hex, base64, base64url, encode and decode. **RFC 4648's full padding ladder verified by running it**: `Zg==`, `Zm8=`, `Zm9v`, `Zm9vYg==`, `Zm9vYmE=`, `Zm9vYmFy`. Decoding refuses bad input rather than guessing. ~~**hex · base64 · base64url** encode/decode~~ | **BUILD** |
| E3 | **DONE v0.0.280 — `lib/secure.bx`**, and the naming is the point: `lib/random.bx` is the SEEDED generator, correct for tests and wrong for keys, and these two must never be confusable. ~~**CSPRNG + `uuid_v4`**~~ | **BUILD, after A1.** Impossible today |
| E4 | **DONE v0.0.280.** CRC-32 promoted out of `tests/pass/bits.bx` into `lib/hash.bx` where callers can reach it, with FNV-1a beside it — and the header says which of these are cryptographic and which are checksums, because using either for a token is a security bug. ~~**Promote CRC-32** out of `tests/pass/bits.bx` into `lib/hash.bx`; add `fnv1a` for a version-stable hash~~ | **BUILD — already written and verified. Cheapest win on the board** |
| E5 | **AES · ChaCha20 · RSA · Ed25519 · X25519 · TLS · Argon2/scrypt/bcrypt** | **BIND — do not hand-roll.** Two reasons: no control over instruction timing, and RSA and the curves need **arbitrary-precision integers**, which do not exist — `Decimal` is a scaled i64 capped at scale 18 |
| E6 | **Secrets cannot be zeroed** — a String lives until its region closes; there is no `zeroise` | **document as a 1.0 limitation** |

**Two naming decisions, to make with the modules rather than after:** `random_from(seed)` never a bare
`random()`, and `string_equals_constant_time` spelled out in full — for the same reason `divide_floor`
and `shift_right_zeros` are, because the *behaviour* is the point.

---

## F — Papercuts

**Found v0.0.283 while fixing B8, and written down rather than folded in:** a bare `it` inside a
string INTERPOLATION in a bracket clause does not resolve — `[it != "v\{it\}"]` is refused with
*unknown variable: it*. The subject is installed for the clause and not re-installed when the lexer
re-enters expression parsing inside a literal. B8's message-rendering fix already handles the case
correctly for when the resolver catches up, so this is one place, not two.


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
| G8 | **The audit recorder — bit-exact replay, on any machine, years later.** Burxt may be the only language that can do this SOUNDLY, and the reason is the list of things it refuses: no floats, no unspecified `Map` order, no threads, and — the one that matters — **effects are DECLARED, so the compiler knows the complete set of things to record.** Capture every file read, command output and clock read into a trace; replay reproduces the run byte for byte on any of the eight targets, because the IR is identical across all of them. `rr` and Antithesis do record-replay at the syscall level: heavy, machine-bound, not portable. Here it is a language guarantee. The pitch is one sentence an auditor understands — *prove this invoice was computed correctly, on a different machine, three years from now*. **Gated on Q1**, which is what makes the effect set askable in the first place |
| G9 | **Currency in the type** — `Decimal<2, USD>`, so adding USD to EUR does not compile. Scale is already a type parameter, so the machinery is mostly there. **Recorded honestly as MISSING rather than as original: F# units of measure did this well years ago**, and the reason to want it here is that this is a money language and the mistake it prevents is the one its users actually make. Needs a ruling on whether the currency is a type parameter or a separate marker |

---

## H — Forcing functions and the release gate

| # | Item |
|---|---|
| H1 | **DISCHARGED — A12 shipped.** The forcing function did its job: the ceiling row it was arguing about turned out to be one quadratic in `self.globals`, and per-block release landed as A12. **A12's forcing function FIRED at v0.0.207**, and the promise was broken once. The ceiling went red in CI at **544 MB against 540**, while passing locally at **537** — the growth cumulative over v0.0.200–207, which added 143 lines to `emit.bx` alone with nothing re-measuring. Raised to **600 against the CI number**, because the 540 was set against a *local* 497 and CI runs ~7 MB higher, so the real margin was 3 MB rather than 43 — the exact mistake the comment above it warns about. **A ceiling must be set against CI, not the laptop.** The raise was taken because a red tree is the failure this project spent thirteen versions learning to avoid and slice 3 is not a hotfix. **CORRECTED v0.0.266, and this row was the roadmap's central argument for A12's priority: this ceiling was never A12's forcing function.** The growth was `self.globals`, a flat String appended once per string literal, quadratic in the compiler's own literal count — see **B29**. Chunking it took peak RSS **1,132 MB → 169 MB** and the rate **61.6 → 9.2 KB/line**, below every historical point on the trend including the 50.1 it started from. So the number that justified calling A12 *the last true blocker* was **one line of a data structure**, and every one of the three bar-raisings was paying interest on it. **A12 could not have fixed it even in principle**: the memory is 96% garbage, but the dead prefixes are interleaved in the arena with the live String still growing, so any release that reclaimed them would reclaim it too. A data-structure bug wearing a lifetime bug's clothes. **A12 keeps its other justifications** — a server loop building temporaries, the LSP re-checking on every keystroke, the freestanding target where the region must be a fixed buffer — and it loses this one. It should stop being argued for with this number |
| H2 | **DONE v0.0.295.** Nine spec headers said *"spec, to implement"* while `spec/README.md` recorded the same milestones as shipped — the two had disagreed for over a hundred versions and **the index was right every time**. Corrected from the index rather than by hand, so the fix cannot invent a third answer. A4.5, A4.6, A4.7, A5.0, A6.0, N2, N5, M1a, A4.4. **A8.0 is genuinely unbuilt and is Andre's decision, so it keeps its header.** `DESIGN.md` restamped v0.0.152 → v0.0.295, and two claims in it were false rather than merely old: its **identity paragraph — the one line most likely to be quoted — still said "opt-in safe inheritance" 249 versions after v0.0.46 dropped inheritance**, and the same file said so two hundred lines below; and *"Open tradeoff — Memory management"* described the fork as unchosen 249 versions after M1 chose regions. **A document that says a decision is still open invites it to be re-argued, which is the expensive half of doc rot — being out of date is only the cheap half.** **Doc hygiene** — six spec headers still say `spec, to implement` for shipped work; `DESIGN.md` is stamped v0.0.152 and its *"Open tradeoff — Memory management"* was decided by M1; `spec/README.md` says *"as of v0.0.58"*; four audit rows are stale; **there is no effects spec in `spec/` at all**. **Fix each in whichever version touches it**, never as a separate cleanup — that is how they rotted |
| H3 | CI green **on the commit being tagged** — a tag on a red commit must be withdrawn, which happened with v0.0.171 |
| H4 | **PASSES — measured v0.0.288, 8.3 s.** `cargo test --release the_release_tarball_works_without_rust_or_llvm -- --ignored` is green: a machine with neither Rust nor LLVM installed can unpack the tarball and compile a program. It is `--ignored` because it builds a tarball, so it is NOT part of the ordinary run and has to be asked for — which is exactly why it needs asking for before a tag rather than assumed. `cargo test --release the_release_tarball_works_without_rust_or_llvm -- --ignored` passes |
| H5 | **DONE — `docs/limitations.md`**, 11 KB, linked from `docs/index.md`, split into decisions that are not coming and gaps with plans. Kept honest since: the debugger section was deleted when C1 landed, the crypto paragraph corrected when it turned out to be two versions stale, and the UTF-8 sentence was found to be a claim the compiler did not enforce — which became B5. **The 1.0 limitations document** — every `Decision` and every unpicked `Blocking` row, so nothing surprises anyone. This is what makes a high bar honest instead of optimistic |
| H6 | **DONE v0.0.295 — `docs/compatibility.md`, linked from the front page.** Most compatibility promises are a paragraph of intent; this one has a command behind it, and the command is `burxt review --semver`. **The rule that makes it enforceable is stated outright rather than left to be discovered: the tool sets the MINIMUM.** It can prove a change is at least a major and prove nothing in the interface broke; **it can never prove an upgrade is safe**, because it reads the interface and not the behaviour — a function with unchanged signature, contracts and effects can still answer differently. A person may always go higher than the tool says, never lower, which is also what makes a 2.0 chosen for a milestone coherent. What is NOT covered is listed as carefully as what is: anything not `public`, the wording of a diagnostic (`--json` is the stable surface), the emitted IR between versions, and the compiler's own internals. And the honest closing: **a promise is worth what the project does the first time keeping it is expensive, and this one has not been tested yet.** A stated **compatibility promise**, with `burxt review` as its mechanical enforcer |
| H7 | **DONE (v0.0.260) — four hosts, not one.** `release.yml` builds natively per architecture: `linux-x86_64`, `linux-arm64` (free on public repos since GA August 2025), `darwin-arm64`, `darwin-x86_64`. `fail-fast: false`, because one broken host must not hide whether the other three work, and `publish` refuses to attach anything unless **four** tarballs arrive — without that count a release would ship one platform and look complete |
| H8 | **DONE (v0.0.260) — a multi-arch OCI image**, `amd64` + `arm64`, from `scripts/Dockerfile`. It **copies** the binaries the matrix already built rather than compiling inside the image: a statically-linked LLVM 18 under QEMU is hours per architecture for a byte-identical result, and a build that slow stops being run. The image carries **gcc**, which is not a convenience — `burxt build` calls `cc` to link, so an image holding only the binary would pass `burxt check` and fail every build, reproducing the exact failure `install.sh` already warns about. It is **run** before it is pushed: build `linux/amd64`, execute `19.99 * 3`, refuse to push unless it prints `59.97` |
| H9 | **DONE (v0.0.260) — Windows, by container, deliberately.** Windows 11's `wslc` runs OCI images natively — no Docker Desktop, no third-party runtime (preview 29 June 2026, GA fall 2026). So H8's image *is* the Windows host, and the native MSVC port is refused with its bill written out in [ROADMAP-1.1](ROADMAP-1.1.md) §W2. The trigger that reopens it: someone who needs `burxt.exe` outside a container |
| H10 | **DONE (v0.0.260) — the release script stopped being Linux-only in a way that could not fail.** `ldd` does not exist on macOS, so the "does this binary link libLLVM?" guard found nothing there and **passed without looking** — for every Darwin build, silently. Now the tool is chosen per platform (`ldd` / `otool -L`) and an unknown platform is a hard stop. *A guard that cannot fail is not a guard.* Also `strip -o` → copy-then-strip, since GNU and BSD disagree about `-o` and a release runner is the wrong place to find out |
| H11 | **Distribution work that needs a machine we do not have → [ROADMAP-1.1](ROADMAP-1.1.md).** Android as a **host** (an experiment with the command written down, not a wall — NDK r27 *is* LLVM 18), the native Windows port, and the `use`-search-path question the container raised. The split is by **verifiability**: 1.0 holds what could be built and proven in one pass; 1.1 holds what cannot be finished by writing it |
| H12 | **The playground — `play.burxt-lang.org`, Andre's, 2026-08-15. Belongs in [ROADMAP-1.1](ROADMAP-1.1.md), which ANOTHER SESSION OWNS — recorded here so the prerequisites are not discovered late, and it needs relaying rather than editing.** The useful finding: **stage-1 does not link LLVM.** Its whole dependency on the outside world is fifteen libc calls — `exit fclose fopen fprintf fread fseek ftell fwrite getrlimit malloc memcpy printf snprintf strcmp strlen` — every one of which `wasi-libc` has. So the compiler is a plausible wasm module today; what it cannot do in a browser is turn its IR text into machine code, which is `llc`'s job. **Phase 1 therefore runs NOTHING and is still the better demo**: `check`, `layout`, `explain memory`, `mcp-schema`, and `review` in two panes showing an agent's deleted precondition. Static files, no sandbox, no server, no security model. **Three prerequisites, and one is ours from today:** the region reserves **4 GB** up front, which is the whole of wasm32's address space (→ the configurable region size G4 already wants, pulled forward by the browser); **`getrlimit` has no WASI equivalent** (B7, v0.0.285, mine); and wasm32 objects emit but do not link, needing a `wasi-libc` sysroot and `wasm-ld`. **Phase 2, running programs, is where Q1 earns its keep**: Andre's host is AWS free-tier EC2, and the 4 GB `malloc` is a RESERVATION that Linux overcommit grants without touching pages — measured peak RSS is 169 MB — so a 1 GB instance is fine, but a playground that executes submitted Burxt hands strangers a shell, because `os_capture` at the top level compiles clean today (→ Q2). `burxt effects --allow nothing` is a one-line security policy checked by a compiler, where Go's playground needs gVisor and Rust's needs a full sandbox — **and it is still belt and braces, not a substitute for the container**, because static enforcement is only as good as the compiler and four silent miscompiles were found in one week |

---

## Not work — decisions on record

Changing these breaks the language. Each has its reason written down, and **all of them belong in H5.**

**Identity:** no floating point — upheld *and strengthened* by N9, where the flagship use nobody thought
was reachable without floats turned out reachable and better without them · no char type, no bare `s[i]`
· no reflection · no inheritance (dropped v0.0.46, *"composition-only is final"*) · no null · no GC, no
refcounting, no runtime · no `unsafe` escape hatch · no truthiness · **no removing Rust** — stage-0 is
the trust anchor and the differential · ~~no file-level privacy~~ **— REVERSED by Andre, 2026-08-15, and it is the
only entry on this list ever overturned, so the reasoning is kept rather than the row quietly edited.**

**The keyword is `public`, spelled out**, because every other name in this language is: `function` not `fn`,
`divide_floor` not `div`, `string_equals_constant_time` in full. `pub` would have been the single abbreviation
in the vocabulary.

**The boundary is the PACKAGE, not the file**, and that is forced by M6 Decision 5 rather than chosen: `use`
concatenates every source into ONE BUFFER, so there is no file boundary at runtime for anything to be private
across. Privacy has to be invented, and the only boundary that will exist is the one C2 creates.

So: **everything is visible within a package; only `public` declarations are importable by a package that
depends on it.** Nothing changes for code written today, because everything today is one package — which is
also why `public` cannot land BEFORE C2. A keyword that parses and does nothing is the "supported versus not
examined" trap this suite was built to refuse, and it would go green on the first try.

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

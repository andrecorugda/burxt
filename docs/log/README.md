# Burxt — the milestone log

Every version, in order, with what it decided and what it cost. The log is a **record**,
not documentation: entries are appended, superseded decisions are marked rather than
rewritten, and a version that was spent on a mistake says so.

It lived in `DESIGN.md` until v0.0.72, when 2,500 of that file's 3,000 lines were log and
finding anything meant searching. The design it serves stayed there;
[DESIGN.md](../../DESIGN.md) is still the place to start.

> **This log covers v0.0.1–v0.0.89.** From v0.0.90 the record moved into the milestone specs in
> [`spec/`](../../spec/), each carrying its own status block — `spec/1.0/M12-STRINGS.md` records M12's
> numbers, `spec/1.0/M13-CONTRACT-SYNTAX.md` records what M13 shipped and what is still pending, and so
> on. That was a reasonable shift, since work became milestone-shaped rather than version-shaped,
> but it happened without a note and this log appeared to simply stop. It did not; look in `spec/`.
> Versions with no milestone of their own are recorded where they live — v0.0.141's wrong answer
> is written into `tests/pass/abi_dyn_record_params.bx` and the invariant that guards it.

## The files

| Versions | What happened | |
|---|---|---|
| **v0.0.1–v0.0.10** | The language runs | [read](01-the-language-runs.md) |
| **v0.0.11–v0.0.20** | Aggregates, dispatch, and the literals money needs | [read](02-aggregates-and-dispatch.md) |
| **v0.0.21–v0.0.30** | Memory, regions, and the first self-hosted pieces | [read](03-memory-and-the-first-self-hosting.md) |
| **v0.0.31–v0.0.37** | The half of a language that lives outside the compiler | [read](04-tooling.md) |
| **v0.0.38–v0.0.42** | `allocates`, `pure`, and the mark | [read](05-allocates-pure-and-the-brand.md) |
| **v0.0.43–v0.0.50** | Contracts, conservation laws, and termination | [read](06-contracts-and-termination.md) |
| **v0.0.51–v0.0.58** | The front end, in Burxt | [read](07-the-self-hosted-front-end.md) |
| **v0.0.69–v0.0.89** | The mark, the shape of the repository, and the fixpoint | [read](08-the-mark-and-the-tree.md) |

## Every entry

### [The language runs](01-the-language-runs.md) — v0.0.1–v0.0.10

- [v0.0.1: the first vertical slice](01-the-language-runs.md#v001-the-first-vertical-slice)
- [v0.0.2: rounding contracts](01-the-language-runs.md#v002-rounding-contracts)
- [v0.0.3: functions, control flow, Bool](01-the-language-runs.md#v003-functions-control-flow-bool)
- [v0.0.4: mutation and loops](01-the-language-runs.md#v004-mutation-and-loops)
- [v0.0.5: checked arithmetic — no silently wrong numbers, ever](01-the-language-runs.md#v005-checked-arithmetic--no-silently-wrong-numbers-ever)
- [v0.0.6: FFI — call into C](01-the-language-runs.md#v006-ffi--call-into-c)
- [v0.0.7: strings — literals only, no heap, no lies](01-the-language-runs.md#v007-strings--literals-only-no-heap-no-lies)
- [v0.0.8: structs — the OOP substrate](01-the-language-runs.md#v008-structs--the-oop-substrate)
- [v0.0.9: hardening — findings from the adversarial review](01-the-language-runs.md#v009-hardening--findings-from-the-adversarial-review)
- [v0.0.10: arrays — fixed-size, always bounds-checked](01-the-language-runs.md#v0010-arrays--fixed-size-always-bounds-checked)

### [Aggregates, dispatch, and the literals money needs](02-aggregates-and-dispatch.md) — v0.0.11–v0.0.20

- [v0.0.11: honest numbers, unary minus, human errors](02-aggregates-and-dispatch.md#v0011-honest-numbers-unary-minus-human-errors)
- [v0.0.12: the aggregate ABI (A4.5)](02-aggregates-and-dispatch.md#v0012-the-aggregate-abi-a45)
- [v0.0.13: receiver methods — the first slice of A4.6](02-aggregates-and-dispatch.md#v0013-receiver-methods--the-first-slice-of-a46)
- [v0.0.14: interfaces and dispatch (A4.6)](02-aggregates-and-dispatch.md#v0014-interfaces-and-dispatch-a46)
- [v0.0.15: `&&`, `||`, `!` — closing A5.0](02-aggregates-and-dispatch.md#v0015-----closing-a50)
- [v0.0.16: string length and equality (A4.4, unblocked half)](02-aggregates-and-dispatch.md#v0016-string-length-and-equality-a44-unblocked-half)
- [v0.0.17: string interpolation, and the syntax-change law](02-aggregates-and-dispatch.md#v0017-string-interpolation-and-the-syntax-change-law)
- [v0.0.18: money and percent literals (A4.7, slice 2)](02-aggregates-and-dispatch.md#v0018-money-and-percent-literals-a47-slice-2)
- [v0.0.19: mixed-scale multiplication — percent-of-money works](02-aggregates-and-dispatch.md#v0019-mixed-scale-multiplication--percent-of-money-works)
- [v0.0.20: sum types and exhaustive matching (A6.0)](02-aggregates-and-dispatch.md#v0020-sum-types-and-exhaustive-matching-a60)

### [Memory, regions, and the first self-hosted pieces](03-memory-and-the-first-self-hosting.md) — v0.0.21–v0.0.30

- [v0.0.21: string bytes, and the first self-hosted piece](03-memory-and-the-first-self-hosting.md#v0021-string-bytes-and-the-first-self-hosted-piece)
- [v0.0.22: the parser self-hosts — and the memory model was not the blocker](03-memory-and-the-first-self-hosting.md#v0022-the-parser-self-hosts--and-the-memory-model-was-not-the-blocker)
- [v0.0.23: regions — M1 slice 1](03-memory-and-the-first-self-hosting.md#v0023-regions--m1-slice-1)
- [v0.0.24: growable arrays + escape checking — M1 slice 2](03-memory-and-the-first-self-hosting.md#v0024-growable-arrays--escape-checking--m1-slice-2)
- [v0.0.25: string concatenation — M1 slice 3](03-memory-and-the-first-self-hosting.md#v0025-string-concatenation--m1-slice-3)
- [v0.0.26: storable trait objects — M1 slice 4, and a corrected claim](03-memory-and-the-first-self-hosting.md#v0026-storable-trait-objects--m1-slice-4-and-a-corrected-claim)
- [v0.0.27: the self-hosted parser is uncapped — M1 complete](03-memory-and-the-first-self-hosting.md#v0027-the-self-hosted-parser-is-uncapped--m1-complete)
- [v0.0.28: reading a file, and rendering a value](03-memory-and-the-first-self-hosting.md#v0028-reading-a-file-and-rendering-a-value)
- [v0.0.29: guaranteed tail calls, and two region bugs found on the way](03-memory-and-the-first-self-hosting.md#v0029-guaranteed-tail-calls-and-two-region-bugs-found-on-the-way)
- [v0.0.30: exactness that survives the boundary (NOVELTY §1, slice 1)](03-memory-and-the-first-self-hosting.md#v0030-exactness-that-survives-the-boundary-novelty-1-slice-1)

### [The half of a language that lives outside the compiler](04-tooling.md) — v0.0.31–v0.0.37

- [v0.0.31: editor support — the half of a language that lives outside the compiler](04-tooling.md#v0031-editor-support--the-half-of-a-language-that-lives-outside-the-compiler)
- [v0.0.32: errors that know where they are](04-tooling.md#v0032-errors-that-know-where-they-are)
- [v0.0.33: a language server](04-tooling.md#v0033-a-language-server)
- [v0.0.34: live diagnostics in VS Code, with no dependencies at all](04-tooling.md#v0034-live-diagnostics-in-vs-code-with-no-dependencies-at-all)
- [v0.0.35: expression spans, sharper carets, and hover](04-tooling.md#v0035-expression-spans-sharper-carets-and-hover)
- [v0.0.36: VS Code speaks to the language server](04-tooling.md#v0036-vs-code-speaks-to-the-language-server)
- [v0.0.37: every mistake at once](04-tooling.md#v0037-every-mistake-at-once)

### [`allocates`, `pure`, and the mark](05-allocates-pure-and-the-brand.md) — v0.0.38–v0.0.42

- [v0.0.38: functions that allocate in the caller's region](05-allocates-pure-and-the-brand.md#v0038-functions-that-allocate-in-the-callers-region)
- [v0.0.39: `pure` — reproducibility the compiler checks (NOVELTY §2, slice 1)](05-allocates-pure-and-the-brand.md#v0039-pure--reproducibility-the-compiler-checks-novelty-2-slice-1)
- [v0.0.40: the brand, in place](05-allocates-pure-and-the-brand.md#v0040-the-brand-in-place)
- [v0.0.41: the mark on `.bx` files](05-allocates-pure-and-the-brand.md#v0041-the-mark-on-bx-files)
- [v0.0.42: a real extension, and a correction](05-allocates-pure-and-the-brand.md#v0042-a-real-extension-and-a-correction)

### [Contracts, conservation laws, and termination](06-contracts-and-termination.md) — v0.0.43–v0.0.50

- [v0.0.43: contracts — `requires` and `ensures`, checked](06-contracts-and-termination.md#v0043-contracts--requires-and-ensures-checked)
- [v0.0.44: conservation laws, checked (NOVELTY §3's headline)](06-contracts-and-termination.md#v0044-conservation-laws-checked-novelty-3s-headline)
- [v0.0.45: `decreases` — termination the compiler checks (NOVELTY §5)](06-contracts-and-termination.md#v0045-decreases--termination-the-compiler-checks-novelty-5)
- [v0.0.46: integer division by name, and inheritance dropped](06-contracts-and-termination.md#v0046-integer-division-by-name-and-inheritance-dropped)
- [v0.0.47: `substring`, allocating methods, and a symbol table in Burxt](06-contracts-and-termination.md#v0047-substring-allocating-methods-and-a-symbol-table-in-burxt)
- [v0.0.48: the escape checker was blind to aggregates](06-contracts-and-termination.md#v0048-the-escape-checker-was-blind-to-aggregates)
- [v0.0.49: the scale rule, enforced by Burxt](06-contracts-and-termination.md#v0049-the-scale-rule-enforced-by-burxt)
- [v0.0.50: `break` and `continue`, earned by evidence](06-contracts-and-termination.md#v0050-break-and-continue-earned-by-evidence)

### [The front end, in Burxt](07-the-self-hosted-front-end.md) — v0.0.51–v0.0.58

- [v0.0.51: the primitives that make a program a tool](07-the-self-hosted-front-end.md#v0051-the-primitives-that-make-a-program-a-tool)
- [v0.0.52: the stage-1 lexer, and it lexes itself](07-the-self-hosted-front-end.md#v0052-the-stage-1-lexer-and-it-lexes-itself)
- [v0.0.53: the stage-1 parser — types, expressions, statements](07-the-self-hosted-front-end.md#v0053-the-stage-1-parser--types-expressions-statements)
- [v0.0.54: stage-1 parses items — and parses itself](07-the-self-hosted-front-end.md#v0054-stage-1-parses-items--and-parses-itself)
- [v0.0.55: the marker words become contextual](07-the-self-hosted-front-end.md#v0055-the-marker-words-become-contextual)
- [v0.0.56: stage-1 follows stage-0, and the cross-check proved its worth](07-the-self-hosted-front-end.md#v0056-stage-1-follows-stage-0-and-the-cross-check-proved-its-worth)
- [v0.0.57: `truncate`, and stage-1 typechecks itself](07-the-self-hosted-front-end.md#v0057-truncate-and-stage-1-typechecks-itself)
- [v0.0.58: stage-1 learns fields, struct literals, builtins and constructors](07-the-self-hosted-front-end.md#v0058-stage-1-learns-fields-struct-literals-builtins-and-constructors)

### [The mark, the shape of the repository, and the fixpoint](08-the-mark-and-the-tree.md) — v0.0.69–v0.0.89

- [v0.0.69: the `b` mark](08-the-mark-and-the-tree.md#v0069-the-b-mark)
- [v0.0.71: the repository root, and why it filled up](08-the-mark-and-the-tree.md#v0071-the-repository-root-and-why-it-filled-up)
- [v0.0.72: the log leaves DESIGN.md](08-the-mark-and-the-tree.md#v0072-the-log-leaves-designmd)
- [v0.0.73: Burxt compiles Burxt](08-the-mark-and-the-tree.md#v0073-burxt-compiles-burxt)
- [v0.0.74: a Run button, and the version the compiler prints](08-the-mark-and-the-tree.md#v0074-a-run-button-and-the-version-the-compiler-prints)
- [v0.0.75: `examples/negative/`, because a folder can carry the promise](08-the-mark-and-the-tree.md#v0075-examplesnegative-because-a-folder-can-carry-the-promise)
- [v0.0.76: examples that teach, and a guide](08-the-mark-and-the-tree.md#v0076-examples-that-teach-and-a-guide)
- [v0.0.77: the Burxt backend learns money](08-the-mark-and-the-tree.md#v0077-the-burxt-backend-learns-money)
- [v0.0.78: enums, `match`, interpolation, C, and `musttail`](08-the-mark-and-the-tree.md#v0078-enums-match-interpolation-c-and-musttail)
- [v0.0.79: 88 of 88 — the Burxt backend compiles the whole language](08-the-mark-and-the-tree.md#v0079-88-of-88--the-burxt-backend-compiles-the-whole-language)
- [v0.0.80: the suite runs on Burxt](08-the-mark-and-the-tree.md#v0080-the-suite-runs-on-burxt)
- [v0.0.81: modules — `use "path"`, and one buffer with a map](08-the-mark-and-the-tree.md#v0081-modules--use-path-and-one-buffer-with-a-map)
- [v0.0.82: the compiler splits into five files, and still compiles itself](08-the-mark-and-the-tree.md#v0082-the-compiler-splits-into-five-files-and-still-compiles-itself)
- [v0.0.83: a standard library, and two rules it uncovered](08-the-mark-and-the-tree.md#v0083-a-standard-library-and-two-rules-it-uncovered)
- [v0.0.84: a tarball someone can use, and the next two milestones specified](08-the-mark-and-the-tree.md#v0084-a-tarball-someone-can-use-and-the-next-two-milestones-specified)
- [v0.0.85: coming from classes](08-the-mark-and-the-tree.md#v0085-coming-from-classes)
- [v0.0.86: the rule that was too strict](08-the-mark-and-the-tree.md#v0086-the-rule-that-was-too-strict)
- [v0.0.87: `write_bytes`, element assignment, and a wall worth naming](08-the-mark-and-the-tree.md#v0087-write_bytes-element-assignment-and-a-wall-worth-naming)
- [v0.0.88: two shorthands, and 239 statements shorter](08-the-mark-and-the-tree.md#v0088-two-shorthands-and-239-statements-shorter)
- [v0.0.89: the compiler counts its own work](08-the-mark-and-the-tree.md#v0089-the-compiler-counts-its-own-work)

## The gap: v0.0.59–v0.0.68, and v0.0.70

These have no entry here, and the reason is deliberate rather than an oversight: they were
ten consecutive versions of **one** milestone — the self-hosted typechecker and then the
backend — and they were recorded in that milestone's own specification while it was being
built, where the running counts (false positives, rejections, lines emitted) belonged next
to the plan they were measured against.

- **v0.0.59–v0.0.68** — [`spec/1.0/M4-SELF-HOSTING.md`](../../spec/1.0/M4-SELF-HOSTING.md), phases
  4b through 5, and §3a for where the bootstrap stands.
- **v0.0.70** — the VS Code extension reinstalled so it carried the new mark; recorded in
  the v0.0.69 entry, since it was the same change finished properly.

Splitting the log made the gap visible, which is the argument for having done it.

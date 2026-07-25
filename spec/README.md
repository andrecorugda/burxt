# Burxt milestone specs — index and status audit

The specs in this folder are the roadmap. Each one is written the same way:
decisions with reasoning, an explicit **must NOT do** section, and a deferred
ledger with the trigger that would earn each deferral a future milestone.

**Read this file first.** The specs were written when Burxt was at roughly
v0.0.1, so several describe work that is now done, and one describes work that
was built in a different order than specified. This index records what is
actually true as of **v0.0.16**, audited by running the compiler — not by
reading the specs. Where a spec and the implementation disagree, the note says
which is right.

## Status at a glance

| Spec | State | What remains |
|---|---|---|
| [A4.4 Strings & Collections](A4.4-STRINGS-COLLECTIONS.md) | **Partial** | Arrays fully done. Strings: literals, printing, FFI, **length + equality (v0.0.16)**. Remaining: `.bytes()`/`.chars()` views; concatenation heap-blocked (M1). |
| [A4.5 Aggregate ABI](A4.5-AGGREGATE-ABI.md) | **DONE** (v0.0.12) | — |
| [A4.6 Interfaces & Dispatch](A4.6-INTERFACES-DISPATCH.md) | **DONE** (v0.0.14) | Traits, `impl`, static + `dyn` dispatch shipped. `class` / `open` inheritance from the North Star is separate and still unbuilt. |
| [A4.7 Signature Grammar](A4.7-SIGNATURE-GRAMMAR.md) | **Not started** | All six deliverables. This is the "eloquence" / demo milestone. |
| [A5.0 Control Flow](A5.0-CONTROL-FLOW.md) | **DONE** (v0.0.3–v0.0.4, v0.0.15) | — |
| [Far-horizon M1–M4](FAR-HORIZON-ROADMAP.md) | **Direction only** | Re-spec each on arrival. M1 (memory model) is the gate. |

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
- `break` / `continue` are still absent. The spec called them a fast follow;
  nothing has needed them yet, so they stay deferred rather than speculative.

### A4.4 Strings & Collections — arrays done, strings half done

**Arrays are complete** (v0.0.10) and match the spec: `[T; N]`, literal
construction, bounds-checked reads, element assignment through a `let mut`
binding, compile-time rejection of a constant out-of-range index, and a
runtime trap naming the index and valid range. `len(a)` is constant-folded.
Not built: the `[0; N]` repeat form (sugar, deferred).

**Strings are only half done** (v0.0.7). Literals with the four escapes,
printing, immutability, and passing to C as `const char*` all work. Missing:

- ~~**Length**~~ and ~~**equality**~~ — **shipped in v0.0.16** as generated
  byte-scan helpers, exactly as this audit predicted. `==` slots into the one
  equality rule; comparison is by bytes, not pointers.
- **`.bytes()` / `.chars()` views** — still not built. Bare `s[i]` is
  correctly absent, per the spec's byte-vs-char decision.
- **Concatenation** — refused, and *correctly* so: it needs allocation, which
  is M1's job.

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

**Hazard to fix before interpolation ships:** `print("hi {name}")` compiles
today and prints `hi {name}` **literally**. Braces are ordinary characters in
a string literal right now, so introducing interpolation *changes the meaning
of existing valid programs* — silently, which is precisely what Burxt refuses
elsewhere. When interpolation lands, `{` in a literal must either become
interpolation or be a compile error demanding an escape (`\{`). It must not
stay ambiguous.

**Note on `$19.99` and inference:** `let price = $19.99;` requires inferring a
binding's type from its initializer. Every `let` in Burxt currently *demands*
an explicit type annotation. So this deliverable quietly introduces local type
inference — a real language change, not just a literal form. It deserves to be
called out as its own decision rather than smuggled in as sugar.

### A4.5 / A4.6 — done, with one ABI correction discovered by building them

Both were implemented directly from their specs and hold their guarantees:
layout is exactly the declared fields with no hidden header (machine-checked),
aggregates pass `byval` and return via `sret`, static dispatch emits no
vtable, and a struct's field offsets are byte-identical with and without
`dyn`.

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
3. **A4.7 Signature Grammar.** The demo milestone, and the biggest remaining
   near-term chunk. Sequence within it: fix the brace hazard, then money and
   percent literals, then `requires` / `ensures` runtime-checked, then
   interpolation. Units and pipelines last — they are the most deferrable.

M1 (the memory model) stays deliberately unopened. Its trigger is "~4+ ledger
entries blocked on ownership" — currently at three (concatenation, mutable
`dyn`, storable/returnable `dyn`), so the gate is close but not met, and every
milestone above stays on the safe side of the heap boundary.

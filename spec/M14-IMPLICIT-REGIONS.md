# Burxt — every block is a region (M14)

> Status: **slices 1 and 2 DONE in both compilers. Slice 3 — per-block release — DONE in stage-0
> (v0.0.272); stage-1 in progress.**
>
> ### Slice 3, measured — acceptance criterion 5
>
> Three programs, and the third is the test. Rows one and three differ by **one line**, so the pair
> isolates escape analysis and nothing else.
>
> | 100,000-iteration loop building a String | before | stage-0 after |
> |---|---|---|
> | value does **not** escape, no `region` | 5,280 KB | **1,408 KB** |
> | the same loop with a hand-written `region each { }` | 1,408 KB | **1,408 KB**, unchanged |
> | value **escapes** (`last = s`) | 5,280 KB | **5,280 KB — stays** |
>
> **An implementation that makes the third row bounded is not a better one; it is a dangling pointer
> that passes the other two.** The criterion as originally written — *"bounded after, unbounded
> before"* — was satisfied by exactly that, which is why it was sharpened before any code was written.
>
> What made slice 3 buildable was not new analysis. The proof obligation *is* the escape analysis, and
> when this was last scoped that analysis had **thirteen holes, eleven of them live use-after-frees**
> (B20–B45). Per-block release on top of it would have freed memory that thirteen routes could still
> reach, with a green suite throughout. It is now 126 of 126 on a 133-program adversarial corpus with
> zero false positives and zero divergence between the compilers.
>
> | | |
> |---|---|
> | `examples/pos/` with every `allocates` deleted | **compiles, same receipt** |
> | Fixpoint rounds, `src/burxt-compiler/main.bx` (8.3k lines, ~500 functions) | **2** |
> | Fixpoint rounds, a 4-deep forwarding chain | **5** — one per link, plus one to confirm |
> | `burxt check src/burxt-compiler/main.bx` | **0.06 s** (3 typecheck passes: 2 probe + 1 real) |
> | stage-1 compiling its own source | **0.14 s** |
> | Suite | 37 invariants, fixpoint intact, stage-1 110 of 110 |
>
> Both timings are the median of three warm runs. **An earlier version of this table said 0.26 s
> for the check, which was a single cold run** — recorded and then repeated without being
> re-measured, which is the failure this project keeps a numbers table to avoid.
>
> The cost is the honest one: checking runs *rounds + 1* times instead of once. Two rounds for
> real code, so ~3×, bounded by the number of signatures. A 20-deep forwarding chain would pay
> 21 passes; the fix if that ever bites is to walk the call graph in reverse-topological order,
> which converges in one for an acyclic program. Not done, because nothing measured needs it.
>
> ### What stage-1 needed that stage-0 did not
>
> Stage-0 runs each probe round on a **throwaway checker**, so no state can leak. Stage-1 has one
> `Unit` and no such option, and the difference cost one real bug:
>
> **A round has to leave the binding stack where it found it.** `check_body` truncates back to
> its own base, so a function body leaves nothing behind — but a **top-level statement** declares
> into the same stack and nothing pops it, because in a single pass nothing ever needed to. The
> second pass met every top-level name already declared and refused **68 of 109 programs** with
> `this name is already declared — Burxt does not shadow`. One `truncate` per round.
>
> Two things made the rest safe, and they are worth naming because they are why this was not
> harder: the `typed` cache is a slot per NODE and the last answer stands, so a second reading
> overwrites rather than appends; and `complain_at` is silenced while probing, which it had to be
> anyway — it **prints**, so without the guard a user would read complaints from a question the
> compiler had not finished answering.
>
> Stage-1 also needed `current_method`. Stage-0 keys methods in a separate set; stage-1 sets the
> `allocates` bit on the **stored signature** — the same place every call site already reads it
> from — so the inference needs no second table and no lookup path of its own, but it does need
> to know which signature to write to.
>
> ### A use-after-free found while testing this, and fixed with it
>
> Not caused by M14 — it was there before, and it produced a **silently wrong answer**:
>
> ```burxt
> function leaked(tag: Int) -> String allocates {
>     region inner {
>         let s: String = "secret-" + to_string(tag);
>         return s;                        // accepted. Printed an EMPTY string.
>     }
> }
> ```
>
> The return rule asks `expr_allocates`, which answers for a concatenation but **not for a name
> bound to one** — a variable read fell through to `false`. So `return "a" + "b"` inside a region
> was refused and `return s` was not, and codegen releases the region before the `ret`.
>
> `tests/fail/allocates_cannot_escape_inner_region.bx` had caught the expression form since the
> beginning, which is exactly what made this hard to see: the case looked covered. The two
> fixtures now sit side by side, one spelling each. The fix records, per binding, whether it
> holds storage from a region *this* function opened — which the checker already computed at the
> `let` to decide whether the `let` needed a region at all, and then threw away.
>
> ### What is NOT done
>
> - ~~**Stage-1 still requires the word.**~~ **DONE in v0.0.144.** `tests/stage0-only/` served its
>   purpose and is gone: `inferred_allocates.bx` moved into `tests/pass/`, where both compilers are
>   held to it, and the temporary invariant that guarded it was deleted — its own comment said to
>   do exactly that.
> - ~~**§5's hole is still open.**~~ **CLOSED in v0.0.143**, and without the syntax §5 proposed —
>   see the note added to §5 below. `tests/fail/allocates_through_a_trait_object_needs_a_region.bx`.
> - ~~Slice 2~~ **DONE in both compilers (v0.0.146): a region is no longer REQUIRED.** See below.
> - Slice 3 — **per-block RELEASE is the only part left.** `allocates nothing` shipped in v0.0.209 and
>   **`burxt explain memory` in v0.0.213** — the latter reporting WHETHER and WHAT, but not §7's third
>   column, WHERE, because that column *is* per-block release. The footer of its own output says so
>   rather than letting the table imply it is complete. Acceptance item 8 is met in the form that
>   matters: the output is generated by running the compiler, and
>   `explain_memory_agrees_with_the_allocation_rule` holds it against `allocates nothing` function by
>   function, so neither can quietly stop consulting the inference. **`allocates nothing` shipped in
>   v0.0.209** (stage-0 checks it; stage-1 parses it and does not, because the allocation fixpoint is
>   stage-0's alone — the same staging slice 1 used two versions apart). ~~Acceptance item 6 is met: a
>   fail fixture per path, direct, through a call, and through a `dynamic`, each naming its cause.~~
>   **Item 6 is NOT met — corrected v0.0.264.** Those three paths have fixtures; a **fourth** does not,
>   and on it the clause is unsound. `function fill(mutable dst: [Int], n: Int) allocates nothing {
>   push(dst, i); }` is **accepted**, and `push` allocates into caller-owned storage. `push` never asks
>   `has_region()`, and `has_region` is the sole recorder, so the owner is never credited; the direct
>   form is caught only because the `let` asks. Recorded as **B22**. Three fixtures covering three
>   paths read as coverage, which is the second process rule exactly: a fixture set cannot tell
>   "refuses everything it should" from "refuses everything anyone thought to write."
>   Slice 2 delivered
>   half of §1: nothing needs a region in order to allocate. It did **not** deliver §3's
>   constant-memory loop, and the numbers below say why that matters.
>
> ### Slice 2, and what codegen settled
>
> `examples/pos/` now compiles with **no `allocates` and no `region`** — the program at the left
> margin — and prints the same receipt. A top-level `let mutable xs: [Int] = [];` works.
>
> The design in §1–§2 assumed this needed escape analysis and destination-passing. It did not,
> because **`burxt.alloc` is a global bump pointer with no region state at all**. Allocation never
> needed a region; a `region` block is purely a RELEASE mechanism — a saved mark and one store to
> put the cursor back. `src/rust-compiler/codegen.rs` has **zero** references to `allocates`. So the whole "there
> is no region open here" family of refusals was never protecting memory: it was asking the
> programmer to name a scope so the compiler could decide where to release.
>
> Slice 2 is therefore a **checker-only** change of about twenty lines per compiler, and it is
> sound by construction for a reason worth stating plainly: **nothing new is released.** Only a
> `region` block releases, and its rule is untouched — a value built inside one still cannot leave
> it (`tests/fail/allocates_cannot_escape_inner_region.bx` and its sibling). Nothing can dangle
> because nothing new is freed.
>
> ### The cost, measured, and it is not small
>
> A loop building 100,000 Strings, peak RSS:
>
> | | |
> |---|---|
> | no region | **5,280 KB** |
> | `region each { ... }` around the loop body | **1,408 KB** |
>
> Memory grows **linearly** without a region. The arena is a 1 GB reservation and exhaustion is a
> named runtime error rather than a crash — but a long-running program that allocates in a loop
> and never opens a region **will** reach it.
>
> So slice 3 is not an optimisation. For a server loop or a batch over a large input it is a
> requirement, and until it lands `region` remains the answer for exactly the case §3 described.
> The keyword is now what it should always have been: **optional, and about release.**
>
> ### Nine fixtures retired, and two ratchets lowered on purpose
>
> Nine `tests/fail` fixtures tested a rule that no longer exists — `slice_needs_region`,
> `string_concat_needs_region`, `substring_needs_a_region`, `interp_value_needs_region`,
> `read_file_needs_region`, `slice_taints_struct`, `allocates_call_needs_a_region`,
> `allocates_method_needs_a_region` and `allocates_through_a_trait_object_needs_a_region` (three
> versions old). `tests/pass/no_region_needed.bx` demonstrates all nine cases instead, so the
> coverage moved rather than vanishing.
>
> Two of the nine were among the 191 stage-1 caught, so the rejection ratchet went 191 → 189, and
> `shape_errors` went 4 → 3 because its fourth error was a `to_string` with no region open. Both
> lowerings are written into `tests/runner.rs` beside the numbers, with the reason — the second
> and third times that floor has ever moved down.

## 0. Where this came from

`examples/pos/` is a four-file point-of-sale, and `examples/pos-python/`, `examples/pos-php/` and
`examples/pos-rust/` are the same app, split the same four ways, printing the same receipt. Written
to be read side by side.

The comparison said Burxt is the shortest of the four in `items` and `tax` — the files that are
about money and types — and loses 24 lines in the two files that are plumbing. It also said this,
which is the part that matters here:

**All three `allocates` in `receipt.bx` are on all three of its functions.** An annotation that
appears on 100% of a file's functions carries no information. It is a chore with no reader on the
other end.

And from Andre, from a developer's seat rather than a compiler engineer's: *"region should be
automatic or not existing at all for developer"*.

## 1. The rule

> **Every `{ }` is already a region, and a value lives in the block that owns the name it is bound
> to.**

Neither keyword survives it. The out-of-the-box step is noticing that **regions are not declared,
they are already there**: a function body, a loop iteration and a file all have a beginning and an
end. The current design makes the programmer name a second time something the block structure
already says.

### What the POS looks like after

```burxt
// today                                          // after
function money_column(...) -> String allocates {   function money_column(...) -> String {
function line_text(...) -> String allocates {      function line_text(...) -> String {
function totals_text(...) -> String allocates {    function totals_text(...) -> String {
function catalogue() -> [Item] allocates {         function catalogue() -> [Item] {
function ring_up(...) -> Int allocates {           function ring_up(...) {

region sale {                                      let shelf: [Item] = catalogue();
    let shelf: [Item] = catalogue();               ...
    ...                                            // 47 lines un-indented, one dead name gone
}
```

Five `allocates`, one `region`, one name (`sale`) never mentioned again, and 47 lines of
indentation. The guarantee is unchanged.

## 2. Decision 1 — ask the destination, not the source

The current rule asks the **callee**: "do you allocate?" That is the wrong end of the call, and it
is precisely why the answer has to be written by hand — a callee genuinely does not know where its
result is going.

The new rule asks a question that always has an answer at the call site:

> A built value goes in the region of **the binding that will own it.**

```burxt
let name: String = label(3);        // owned here            → this block
print(label(3));                    // owned by nobody       → dies at the semicolon
push(all, label(3));                // owned by `all`        → `all`'s block
```

This is not new machinery. It is what `allocates` **already means** — "builds in the caller's
region". M14 makes it the *only* rule, and once it is universal there is nothing left to declare.

### Decision 2 — when in doubt, promote outward

Where the destination cannot be determined, the value goes in the **longer-lived enclosing block**.
That can only ever cost memory; it can never dangle. For a language whose thesis is correctness,
that is the right direction to be wrong in.

The known ambiguous case is a store reached through a `dynamic` — the destination is behind a
vtable, so no analysis at the call site can see it. Promote to the caller's block: sound, and
strictly better than today's answer (§5).

## 3. It also releases sooner, which was not the goal

Today every allocation in the POS lands in the single open region `sale`, and a region releases at
its end. So `ring_up`'s loop:

```burxt
while i < len(sale) {
    print(line_text(rule, line));    // a String, into `sale`
}
```

builds one String per line and **holds every one until the program ends.** Three lines: invisible.
Ten thousand: ten thousand live Strings.

Under M14 that String's destination is the **loop body**, released at each closing brace. The loop
becomes constant-memory with no source change. The current design is not merely more verbose — on
this exact program it holds memory it has no use for.

This must be measured, not asserted: acceptance item 5.

## 4. The top level becomes a real scope

```burxt
let mutable xs: [Int] = [];    // today: error, needs a region; there is none open here
```

The top of a file is not a region today, so it cannot hold anything that lives in one. Under M14 a
file is a block, so it is a region, and that line works. *"No entry point to declare"* stops being
a demonstration and becomes somewhere a program can actually be written — which answers Andre's
*"having no main is good, but bad at the same time"*.

## 5. A hole this closes by construction

Found while answering "why do we need `allocates`":

**A trait signature cannot declare `allocates`** — `parse_trait_sig` has no field for it. So the
check fires for a direct call and is silently skipped through an interface object:

```burxt
// direct call, no region open, caller not `allocates`:
error: `describe` is declared `allocates`, so it builds its result in the caller's region
       — and there is none open here.

// the identical call through `dynamic Describable`: compiles, runs, no error.
```

Nothing was corrupted in the reproduction only because a region happened to be open further up the
stack. The check did not fire. M14 removes the hole by removing the declaration: nothing is declared,
so nothing can fail to be.

### Closed in v0.0.143 — and the fix proposed here was the wrong one

Fixed, but **not** by putting `allocates` on trait signatures as this section originally proposed.
That design was worse and slice 1 is what made it unnecessary.

`allocates` on an interface signature is **one fact in two places**: the interface would declare it and every
implementation would have to agree, with nothing but a check keeping them in step. That is precisely
the failure `spec/A7.0-NAMING.md` exists to prevent, and it would have added a keyword to a position
where the language currently has none.

Now that `alloc_methods` is INFERRED, the answer can simply be read off the implementations:

> A method reached through an interface object allocates if **any** implementation of it does.

Conservative because it must be — a call through `dynamic T` cannot know which implementation runs
— and it costs a region the program was going to need anyway. **No syntax at all**, and nothing for
a programmer to keep in step.

The same rule now covers a value of a type PARAMETER, asked of its bound: `show<T: Describable>`
calls `T`'s methods through the interface exactly as an interface object does.

One implementation detail worth recording, because getting it backwards silently ruins the
inference: `leaks` is computed **before** asking `has_region`. Under probing `has_region` *records*
that something wanted a region, so asking it first would record on every such call and mark half the
program as allocating.

## 6. The annotation inverts, and becomes opt-in

Nobody needs "this allocates" — in `receipt.bx` that is 3 of 3. What a hot loop or a small target
needs is the **promise that it does not**:

```burxt
function tick(state: Int) -> Int allocates nothing {
    ...        // a compile error if this, or anything it calls, allocates
}
```

Reads as English, reuses a word already in the language, and appears **only where it says
something**. The same shape as `pure`: a checked claim, opt-in, on the few functions that mean it.

`allocates` alone stays **legal but optional** — accepted, verified against what is inferred, never
required. Every existing program keeps compiling, so M14 is additive exactly as M13 was.

## 7. `burxt explain memory` replaces the annotation

The honest cost of inference is that the memory story leaves the source. Today `allocates` is
readable; afterwards you trust the compiler. The answer is not to keep the annotation — it is to
make the fact **queryable**, because it is wanted occasionally rather than always:

```
$ burxt explain memory examples/pos/till.bx
till.bx:17    catalogue()   → 3 Items, 1 array    file block
receipt.bx:29 line_text()   → 2 Strings           ring_up loop body, released per iteration
till.bx:44    ring_up()     → nothing
```

Strictly more than `allocates` ever said — *where* and *how much*, not just *whether* — and present
when asked rather than on every signature forever.

## 8. What this costs

1. **This is escape analysis, in two compilers.** Stage-0 has most of the pieces: `expr_allocates`
   already walks bodies, and the escape rules already know which types are region-allocated. New is
   the fixpoint over the call graph and the destination propagation. **Stage-1 is the hard half**,
   because it names every binding by its SPAN in the source — the same wall that stopped M10's `for`
   desugar and M13's return bracket. Expect the staging M7 used: stage-0 first, stage-1 after.
2. **Conservative promotion can surprise.** A value the analysis cannot place lives longer than a
   reader would guess. §7 is the mitigation, and "held too long" is the right failure direction.
3. **A teaching moment is lost.** `allocates` announces that this language has a memory model. The
   counter-evidence is `receipt.bx`: on every function, it teaches nothing. `docs/guide/04-memory.md`
   and §7 teach it better.

## 9. Acceptance

> **Status of these nine, measured v0.0.263 before slice 3 began — five are already met, and that
> narrows what slice 3 actually owes.** Slices 1 and 2 delivered the *ergonomic* half of M14; what
> remains is the *memory* half, and only that.
>
> | | criterion | state |
> |---|---|---|
> | 2 | `examples/pos/` with zero `allocates`, no `region`, same receipt | **already met.** Every remaining mention of either word in `examples/pos/*.bx` is in a **comment** — there is no `allocates` keyword and no `region` block left in that code. Receipt recorded for the diff |
> | 4 | top-level `let mutable xs: [Int] = [];` | **already met** — compiles and runs today |
> | 6 | `allocates nothing` refuses transitively | **NOT MET — I was wrong two commits ago.** Shipped v0.0.209 and **unsound through a `mutable` parameter**: `fill(mutable dst: [Int], …) allocates nothing { push(dst, i); }` is accepted. Recorded as **B22**. I marked this met by reading "shipped" instead of measuring it, which is the third time today |
> | 7 | the §5 hole has a fail fixture | shipped |
> | 8 | `burxt explain memory` | shipped |
> | 1 | fixpoint intact | **regression guard** |
> | 3 | every `tests/pass` compiles unchanged | **regression guard** |
> | 5 | the three measurements below | **THE deliverable** |
> | 9 | `docs/guide/04-memory.md` rewritten | **the deliverable's other half** |
>
> So slice 3 is not "nine things." It is **one behaviour change measured three ways, plus a page**,
> with two regression guards standing over it. That is worth writing down because the row count made
> it look larger than it is, and an item that looks larger than it is gets deferred — which is what
> happened to this one for fourteen versions while its ceiling crept to a 0.53% margin.
>
> It does **not** make the work smaller. Escape analysis in two compilers is still the hardest thing
> on this roadmap, and §10 still governs. It makes the *target* smaller, which is a different and
> more useful fact.

1. Both compilers, **byte-identical fixpoint intact**, staged.
2. `examples/pos/` compiles with **zero** `allocates` and **no** `region` block, producing the same
   receipt — the same test that proves the ergonomics proves the semantics.
3. Every existing program in `tests/pass/` compiles **unchanged**, `allocates` and `region` included.
   If a single one has to change, M14 stopped being additive and this spec is wrong.
4. A top-level `let mutable xs: [Int] = [];` compiles (§4), with a pass fixture.
5. **Measured**, not claimed: a loop building 10,000 Strings has bounded peak RSS under M14 and
   unbounded before it (§3). Recorded in this spec's status block as M12 recorded its numbers.

   **Sharpened before building, v0.0.263, because as written this criterion passes for a version of
   slice 3 that corrupts memory.** "Bounded after, unbounded before" is satisfied by an analysis that
   releases *everything* at the end of every block — which is exactly the use-after-free this feature
   can produce. The criterion needs a **control that must NOT change**, so three programs are measured,
   not one. Baseline taken on the machine of record before any code changed:

   | program | today | required after slice 3 |
   |---|---|---|
   | 100k-iteration loop, `let s: String`, **value does not escape** the body | **5,280 KB** | **~1,408 KB** — this is the win |
   | the same loop with a hand-written `region each { }` | **1,408 KB** | **unchanged** — slice 3 must not regress what already works |
   | the same loop where the value **escapes** (`last = s`, an outer binding) | **5,280 KB** | **still 5,280 KB** — Decision 2, promote outward |

   **The third row is the whole test.** A slice 3 that makes it bounded is not a better slice 3; it is
   a dangling pointer that passes criterion 5 as previously written. The first and third programs
   differ by one line — `last = s` — so the pair isolates escape analysis and nothing else, which is
   the property a measurement needs to mean anything.
6. `allocates nothing` refuses a function that allocates, including transitively, with a fail
   fixture per path — direct, through a call, and through a `dynamic`.
7. The §5 hole has a fail fixture: an allocating interface method called with no region open is refused.
8. `burxt explain memory` exists and its output is generated by running the compiler, like
   `docs/examples/index.md` — a test regenerates and diffs it.
9. `docs/guide/04-memory.md` rewritten in the same commit. The current page teaches a chore that no
   longer exists.

## 9b. Decision 3 — ALL-OR-NOTHING PER BLOCK (settled v0.0.272, before building)

**A bump allocator is LIFO and destination propagation is not.** That is the one genuinely hard fact
in slice 3, and it is the allocator's shape rather than missing code. "Allocate this value into the
ENCLOSING block" means placing it *below* the current block's mark while that block is still
allocating *above* it. The instant one value in a block escapes outward, `store next, mark` frees it
along with everything else.

Three ways out. Naming the choice before building it, per the §A7d precedent:

1. **ALL-OR-NOTHING PER BLOCK — chosen.** A block releases only when the analysis proves that
   *nothing allocated inside it escapes*. Sound, needs **no allocator change**, and a block that
   fails the proof simply does not release — which is today's behaviour, and therefore what makes
   acceptance item 3 (*every existing `tests/pass` program compiles unchanged*) reachable at all.
   It still delivers §3's headline, because the common case is a loop body building a String that
   nobody keeps.
2. Two cursors, or a promotion arena. Changes `burxt.alloc`'s signature and edges toward the runtime
   bookkeeping §10 forbids.
3. Copy-down on block exit. Breaks every pointer already handed out. No.

**Why this is now buildable when it was not in August.** The proof obligation in (1) is exactly the
escape analysis that B20–B37 built: *does any value allocated in this block reach a binding declared
outside it* — through assignment, growth, a field, an element, a `return`, a `mutable` parameter, a
`dynamic`, a pattern binding, a field read, or a relay. Slice 3 does not need a new analysis. **It
needs the analysis that now exists, asked once per block instead of once per assignment.**

That is also why closing that family first was not a detour: without it, per-block release would have
freed memory that thirteen separate routes could still reach, and each one is a use-after-free the
suite could not see.

**What stage-0 needs that stage-1 does not.** Stage-1's taint lives on the `Binding` record, which
already carries `depth`, and `check_block_nodes` already pops by depth — block ownership is a field
read. Stage-0's is two flat `HashSet<String>` cleared wholesale, so it must become depth-aware before
nesting can be allowed at all. **M14 §8 has this backwards**: it calls stage-1 "the hard half" on the
strength of span-named bindings, and spans identify *names* while slice 3 places *allocations*.

## 10. What this must NOT do

- **NO garbage collector, no reference counting, no runtime.** The guarantee after M14 is identical
  to the guarantee before it. If any part of this needs bookkeeping at run time, the design is wrong.
- **NO removing the escape rule.** A value still cannot outlive its block. What changes is that this
  stops being a memory rule to learn and becomes ordinary lexical scoping, which every developer
  already knows.
- **NO breaking `region` or `allocates`.** Both stay legal. §6.
- **NO inferring across `external function`.** No body, nothing to infer from; the declaration
  stays required there.
- **NO guessing inward.** An unplaceable value is promoted to the LONGER-lived block, never the
  shorter one. Decision 2 — a wrong guess must cost memory, never correctness.

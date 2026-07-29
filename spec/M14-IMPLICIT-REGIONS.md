# Burxt — every block is a region (M14)

> Status: **slice 1 DONE in stage-0 (v0.0.142) — `allocates` is inferred. Slices 2–3 pending.**
>
> | | |
> |---|---|
> | `examples/pos/` with every `allocates` deleted | **compiles, same receipt** |
> | Fixpoint rounds, `examples/stage1.bx` (8.3k lines, ~500 functions) | **2** |
> | Fixpoint rounds, a 4-deep forwarding chain | **5** — one per link, plus one to confirm |
> | `burxt check examples/stage1.bx` | **0.26 s** (3 typecheck passes: 2 probe + 1 real) |
> | Suite | 38 invariants, fixpoint intact, stage-1 still 109 of 109 |
>
> The cost is the honest one: checking now runs *rounds + 1* times instead of once. Two rounds
> for real code, so ~3×, and it is bounded by the number of functions. A program with a 20-deep
> forwarding chain would pay 21 passes; the fix if that ever bites is to process in
> reverse-topological order, which makes an acyclic call graph converge in one. Not done, because
> nothing measured needs it.
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
> - **Stage-1 still requires the word.** Slice 1 is stage-0 only, staged the way M7 staged
>   generics. That is why the proof lives in `tests/stage0-only/` and not `tests/pass/`: that
>   directory's contract is that both compilers accept everything in it, and an exemption list on
>   the 109-of-109 equality would be worse than a separate directory — a floor with holes cannot
>   see a regression.
> - ~~**§5's hole is still open.**~~ **CLOSED in v0.0.143**, and without the syntax §5 proposed —
>   see the note added to §5 below. `tests/fail/allocates_through_a_trait_object_needs_a_region.bx`.
> - Slices 2 and 3: implicit block regions, `allocates nothing`, `burxt explain memory`.

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
check fires for a direct call and is silently skipped through a trait object:

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

`allocates` on a trait signature is **one fact in two places**: the trait would declare it and every
implementation would have to agree, with nothing but a check keeping them in step. That is precisely
the failure `spec/A7.0-NAMING.md` exists to prevent, and it would have added a keyword to a position
where the language currently has none.

Now that `alloc_methods` is INFERRED, the answer can simply be read off the implementations:

> A method reached through a trait object allocates if **any** implementation of it does.

Conservative because it must be — a call through `dynamic T` cannot know which implementation runs
— and it costs a region the program was going to need anyway. **No syntax at all**, and nothing for
a programmer to keep in step.

The same rule now covers a value of a type PARAMETER, asked of its bound: `show<T: Describable>`
calls `T`'s methods through the trait exactly as a trait object does.

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

1. Both compilers, **byte-identical fixpoint intact**, staged.
2. `examples/pos/` compiles with **zero** `allocates` and **no** `region` block, producing the same
   receipt — the same test that proves the ergonomics proves the semantics.
3. Every existing program in `tests/pass/` compiles **unchanged**, `allocates` and `region` included.
   If a single one has to change, M14 stopped being additive and this spec is wrong.
4. A top-level `let mutable xs: [Int] = [];` compiles (§4), with a pass fixture.
5. **Measured**, not claimed: a loop building 10,000 Strings has bounded peak RSS under M14 and
   unbounded before it (§3). Recorded in this spec's status block as M12 recorded its numbers.
6. `allocates nothing` refuses a function that allocates, including transitively, with a fail
   fixture per path — direct, through a call, and through a `dynamic`.
7. The §5 hole has a fail fixture: an allocating trait method called with no region open is refused.
8. `burxt explain memory` exists and its output is generated by running the compiler, like
   `docs/examples/index.md` — a test regenerates and diffs it.
9. `docs/guide/04-memory.md` rewritten in the same commit. The current page teaches a chore that no
   longer exists.

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

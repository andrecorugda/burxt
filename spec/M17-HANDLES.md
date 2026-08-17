# M17 — a value a host can hold between calls

**Status: specified, not built.** Andre ruled the mechanism on 2026-08-18: **a table, not trust.**

## The problem, in one program

A browser calls into a compiled Burxt module on every click. Between two calls there is nowhere for
the application's state to be:

```burxt
class Model { route: Route, items: [Item], draft: String }

// The host wants to do this, and cannot:
//     let h = bx.init();
//     h = bx.update(h, message);      // …later, from an event handler
//     element.innerHTML = bx.view(h);
```

Today the state crosses the boundary **as text** on every event. That costs a serialisation per
keystroke, which is the visible half. The half that matters is that it comes back as `Json` and
something must hand-validate it into a `Model` — so the checking property is surrendered precisely
where an application works hardest, in the layer built to demonstrate it.

## What is NOT the problem — measured, and it corrects both package sessions and me

Every one of these was believed to be an obstacle and is not:

| believed | measured |
|---|---|
| the region must survive the call that created it | **it already does.** A `Release` is placed only on a block that provably keeps nothing; a function that hands storage back is tainted and gets none. The arena resets only inside an explicit `region` block. |
| memory does not persist between wasm calls | it does. `examples/wasm/host.mjs:47` — *"Burxt asks for its region ONCE… `free` is never called and never imported"* |
| a module can only have one entry point | `--export=main --export='bx.island'` already works |
| pointers are not stable | they are; the allocator is a bump over linear memory |

**So this is a type-system and boundary problem, not a memory one.** That is the finding that makes
it a small milestone rather than a large one, and it was only visible after asking what the compiler
does rather than what the notes said.

## The decision: a table

The host holds an integer. Something has to turn that integer back into a `Model`, and there are two
ways to do it.

**Trust it.** One line, and it puts a second hole in the pointer wall — a wrong integer becomes type
confusion with no diagnostic. `c_bytes_at`'s length is the precedent and its own comment calls that
*"the wall's one soft edge"*.

**A table.** The compiler keeps live handles; the host gets an index; an index that is not live is a
**named refusal**. Costs a table and a lookup.

**Andre's ruling: the table.** *"We are Burxt, we do things right."* Everything else in this language
refuses rather than trusts, and a second exception would be the one that undoes the first as a
principle rather than an incident.

### An index alone is not enough — it needs a generation

An index catches out-of-range. It does not catch the case that actually happens: a host that kept a
handle after it was superseded.

```
    h1 = bx.init()          // slot 0, generation 1
    h2 = bx.update(h1, m)   // slot 0, generation 2 — h1 is now stale
    bx.view(h1)             // must REFUSE, and slot 0 is live, so an index check passes
```

So a handle is `(index, generation)` packed into one integer, and the table stores the generation it
last issued. A stale handle is the silent use-after-free this whole milestone exists to avoid, and it
is exactly the case a naive table misses.

**The refusal must name both facts**, for the reason `std/`'s two messages already record — *a check
that cannot tell two failures apart sends the reader to the wrong one*:

```
error: this handle refers to a Model that was replaced by a later `update`.
       It was issued at generation 2 and the live one is generation 5.
```

which is a different problem, and a different fix, from:

```
error: this handle was never issued by this module.
```

## Reclamation — and the answer is already in the language's vocabulary

Nothing is reclaimed. A UI calls `update` on every keystroke, each one builds a new `Model`, and the
table tracks *liveness* rather than freeing *memory*. A form typed into for a minute allocates a model
per character and frees none.

**The prior art all answers a question this language declined.** Alpine, Vue and Svelte keep state as
a JavaScript object and let the garbage collector own its lifetime — they do not have this problem
because nothing crosses a boundary. Elm has the identical `update : Msg -> Model -> Model` shape and
also compiles to JS, so also has no boundary. The projects that *do* share our situation —
`wasm-bindgen` and Emscripten's Embind — both chose **explicit release**, a `.free()` on the host
side, and both accept the leak when a host forgets.

Every one of those is an answer to *"when is this value dead?"*, which needs tracking. Burxt's memory
model exists because it refuses that question and asks *"when is this batch of work over?"* instead.
Importing GC or `free()` imports the premise we rejected.

**For a UI, the batch of work has a name: a frame.** One event, one update, one render, done.
Everything allocated during a frame is dead at its end — except the new model. That is not a new
concept, it is `allocates`, which already means *the storage belongs to the CALLER's region, so it
outlives the call*. The host is the caller.

    arena P — the host's. Holds exactly what outlives a call: the model.
    arena F — the frame's. Scratch, intermediate Html, parsed strings, everything else.

    `update` runs with F open. Everything it builds goes in F, except its result, which
    `allocates` into P. The frame ends and F resets to zero — O(1), the guarantee a region
    already makes.

No `free()`, no finalizer, no GC, and **the host cannot forget because it was never the host's job.**
That is the whole difference from the prior art: `wasm-bindgen` makes lifetime the host's
responsibility, which is the thing this memory model spent its existence refusing.

### What actually stands in the way, measured

**`allocates` is implemented as "do not free", not as "allocate in the caller's region."** Those are
the same thing when the caller is a Burxt frame further down the stack — the storage already sits
below the mark, so declining to free it suffices. They are **different things when the caller is a
host**, because there is no enclosing Burxt frame for the value to belong to. The semantics name a
place the runtime does not have.

    codegen.rs:83   "(heap base, bump cursor) globals for region allocation"   — ONE cursor
    build_region_open                                                          — saves and restores it

So the change is: give `allocates` the second cursor its own definition already implies. Measured
blast radius in stage-0 — **every allocation funnels through one function**, which is why this is a
contained change rather than a rewrite:

    heap_globals call sites   3        region_marks references   11
    alloc_fn call sites       3        region open/close          8

`emit.bx` shows 81 matches, but most are IR text rather than decision points; the real count wants
measuring before the work is scheduled.

### The tension, which is the same defect one level up

```burxt
function dispatch(m: Model, message: Message) -> Model {
    if nothing_changed { return m; }     // returns the OLD model, which already lives in P
    ...
}
```

That branch **relays** its parameter. If P is ever swapped or compacted, the returned handle points at
the previous contents — which is `dbc9241`'s use-after-free at architecture scale, the same
substitution of *what a function did* for *what the value is*.

**And the compiler already knows.** `relay_params` records, per function, whether a result may point
at a parameter; it was corrected on 2026-08-17 and `the_two_compilers_format_the_same_way`'s sibling
tests hold it. So the boundary can distinguish a fresh value in P from an alias of the one already
there, and treat them differently rather than guessing.

That fact is why this design suits Burxt specifically and would not transplant. No other language in
this space has *does this result alias its input* available as a compile-time fact — and it is the
same asset that makes `burxt review` possible: **the interesting property is in the signature, so a
tool can read it.**

### Left to decide before code

- Two cursors, or one cursor and a compacting move of the model between frames? The first is simpler
  and doubles the reservation; the second keeps one arena and has to rewrite interior pointers, which
  this language has no machinery for. **Two cursors, unless the reservation is the objection.**
- What the boundary does when `update` relays rather than builds. Refuse it, or detect it and skip the
  reset for that frame? Refusing is honest and costs the `return m` branch every UI wants to write.
- Whether `view(h)` borrows without consuming, which it must, and what that means for the generation
  check.

## Acceptance

- [ ] A host holds a value across two calls and the second sees a typed `Model`, not `Json`.
- [ ] A stale handle is refused, naming the generation it held and the generation that is live.
- [ ] A never-issued handle is refused with a *different* message.
- [ ] A handle from one module is refused by another.
- [ ] Both compilers, byte-identical, held by a test that compares them rather than each to a fixture.
- [ ] A fail fixture per refusal, and the reclamation answer written down before the first line of
      code — this file exists because *"what does a host-owned value mean"* is the sentence that gets
      re-derived wrongly later.

## Why this is worth a milestone

`burxt review` can diff what a state transition promises between versions. **No other framework can
offer that**, because no other framework puts application state where a tool can see it — it is in a
closure, a store, or a JSON blob. A handle keeps it in the type system.

That is also the argument against the alternative both package sessions raised and both rejected: a
mutable global closes the same hole and takes the state *out* of the type system, which is the one
thing that would make the feature pointless while appearing to deliver it.

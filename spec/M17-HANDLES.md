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

### The mechanism: record the mark, and let the frame dispel

Andre's, 2026-08-18, and it replaces the two-arena sketch above because it needs **one** strip rather
than two — which is the constraint that actually bites, since WebAssembly has about 4 GB of address
space in total and a program reserves 1 GB today.

    frame_start = marker                  // record the pointer
       … the frame runs, allocating freely …
    new_model = update(msg, model)        // built somewhere above the mark
    copy_down(new_model, frame_start)     // what it learned returns to the original
    marker = frame_start + size           // dispel: everything else vanishes at once

A frame is a clone. It works in its own space, and when it dispels **everything it made vanishes
except what it learned**, which returns to the original. The reset is the same O(1) move a region
already makes; the only addition is carrying one value back across it.

**Copy down only what lies ABOVE the mark.** Anything below already survived the previous dispel and
does not move, so its pointers stay valid. If a keystroke changed one field and left the item list
alone, that list is not copied — the new model simply points at where it already is. The cost is
*what this frame made and kept*, never *the whole application state*, which is the difference between
this and serialising: the text approach pays for everything on every event.

**This is writable because the compiler already knows the layout.** `burxt layout` prints, for every
class, its size and the offset and type of every field:

    Model: size 40 align 8
      +0  Int      (8 bytes)      — a value, nothing to follow
      +8  [Item]   (24 bytes)     — a slice: pointer, length, capacity
      +32 String   (8 bytes)      — a pointer into the arena

So `copy_down` is a **per-type function the compiler generates** from a table it already computes. No
tracing, no roots, no runtime type information, no scanning of anything — the opposite of a
collector. It walks exactly one value, following exactly the fields that are pointers, and only where
they point above the mark.

**And it dissolves the relay tension rather than ruling on it.** The previous sketch needed a decision
about `if nothing_changed { return m; }`, because a returned old model would have pointed into an
arena about to be reset. Here the result is copied down whichever way it was produced, and a pointer
already below the mark is left exactly where it is. There is nothing to refuse and no rule to
remember — which is the sign the mechanism is the right shape rather than a patch over the wrong one.

### Left to decide before code

- **The reservation in a browser.** One strip means no doubling, but a 1 GB reserve is still half of
  wasm32's address space. A UI's model is small; the figure should be a build-time choice rather than
  the compiler's own number reused.
- **What `copy_down` does with a cycle.** A model cannot contain one today — there is no way to build
  a cycle in Burxt's value types — but that is a property worth asserting rather than assuming, since
  the walker would not terminate if it were ever untrue.
- **Where the mark lives across a call.** The host holds a handle; the runtime holds the mark that
  handle's frame began at. That is one more thing the table stores, and it is the natural home for it.

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

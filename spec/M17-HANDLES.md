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

## Open — the question the table does not answer

**Nothing is reclaimed.** A UI calls `update` on every keystroke and each one builds a new `Model`.
The arena is a bump allocator, so a form typed into for a minute allocates a model per character and
frees none. The table tracks *liveness*; it does not free *arena memory*.

This is the real remaining design work, and it is where bmx's instinct was pointing even though the
obstacle they named turned out to be solved. Three shapes, none chosen:

1. **Explicit `release(h)`.** Simple, and it is a leak the day a host forgets — which is the failure
   mode this language declines to accept everywhere else.
2. **`update` consumes its handle.** The old generation dies at the call, so the table always holds
   exactly one live model and the compiler knows when the previous one became garbage. Safe, and it
   makes the signature say so. The awkward half is what `view(h)` does — it must borrow without
   consuming.
3. **Accept the growth and state a ceiling.** Honest, and wrong for the one application anybody wants
   to write.

**(2) is the one worth designing first**, because it is the only one where the language rather than
the host is responsible — and because a consumed handle is a fact a signature can carry, which is
this language's answer to every other question of this shape.

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

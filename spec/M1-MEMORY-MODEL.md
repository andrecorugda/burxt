# Burxt — M1 Memory Model Specification

> Status: **decided, to implement.** This is the pivotal fork the far-horizon
> roadmap flagged: *"the one decision most likely to determine adoption... do
> NOT decide it casually or early."* It is no longer early — five ledger
> entries were blocked on it — and it is not being decided casually.

## 0. The four decisions taken (2026-07-25)

| Question | Decision |
|---|---|
| Memory model | **Regions**, as the unit of ownership |
| Data races as compile errors | **Must-have** — a headline guarantee |
| Scope of this milestone | Spec, then implement fully |
| What Burxt is for | **Both — the callable exact core first**, the service language second |

Two of those look contradictory, and resolving that honestly is what this spec
is mostly about.

## 1. The tension, and the synthesis

The choice was framed as *regions OR ownership*, and on that framing the two
answers conflict: plain region allocation cannot make data races compile
errors, because nothing stops two threads reaching into the same region.

**But that framing was too narrow.** Rust makes ownership work at *object*
granularity. Nothing requires that. The synthesis:

> **A region has exactly one owner at a time. Ownership transfers at region
> granularity, not per object.** Everything inside a region is therefore
> reachable by exactly one thread — so data races are impossible *by
> construction*, with no per-object borrow checking at all.

Inside a region you may alias freely, mutate freely, even build cycles — none
of it can race, because no other thread can see the region. Sharing across
threads is explicit: transfer the region (the sender loses access), or use a
region marked shared, which requires synchronization.

**This is real prior art, and it is not mainstream** — the same shape as the
effects finding in `NOVELTY.md`. **Project Verona** (Microsoft Research) is
built precisely on concurrent ownership of regions rather than objects.
**Pony** achieves data-race freedom through reference capabilities without a
Rust-style borrow checker. Enough precedent that this is not a fantasy; little
enough adoption that there is room.

**Why this fits Burxt specifically, better than Rust's model would:**

- **The granularity matches the workload.** A money system's natural unit is a
  transaction. "One region per transaction, handed to one worker" is exactly
  region-granular ownership. Per-object borrows would be solving a problem
  Burxt does not have.
- **It is dramatically less ceremony than Rust.** No lifetimes in signatures,
  no `&`/`&mut` on every parameter, no borrow errors on ordinary code. That
  serves the "easy" half of the North Star's permanent tension, which full
  ownership would have cost dearly.
- **Cycles just work.** Inside a region, aliasing and cycles are fine —
  removing the one problem ARC could never solve, without a collector.
- **It is buildable by one person.** Region-granular ownership is a far
  smaller checker than per-object borrow tracking.
- **It serves the core-first target.** A library called from Node wants
  "allocate during the call, release on return" — which is a region per call.
  Decision 4 and decision 1 reinforce each other.

**What it costs, stated plainly:** coarser granularity is genuinely less
expressive than Rust. You cannot hand one object from a region to another
thread while keeping the rest — you transfer the whole region or copy the value
out. For Burxt's transaction-shaped workloads that is the right trade; for
fine-grained shared-memory parallelism it would not be. That is the honest
limit of this model, and it is accepted deliberately rather than discovered
later.

## 2. The model

- **A region is a named allocation scope.** Values allocated inside it live
  until the region ends, then are released as a unit in O(1) — no per-object
  free, no refcount, no tracing.
- **Every heap value belongs to exactly one region**, fixed at allocation.
- **A region has one owner.** Initially the thread that opened it.
- **A value may not outlive its region.** Enforced at compile time: returning
  or storing a reference to region data beyond the region's end is an error
  naming the region.
- **Copying out is always allowed.** Scalars, and aggregates by value, may
  leave a region freely — they are copies, per A4.5's value semantics, which
  this does not disturb.
- **Cross-region references are refused** in the first cut. A reference always
  points within its own region.
- **Concurrency (later, but designed now):** transferring a region to another
  thread moves ownership; the sender can no longer reach it. That is the whole
  data-race story — no locks required for owned regions, and no races
  possible.

Sketch, not final syntax:

```text
region tx {
    let mut entries: List<Entry> = List.new();
    entries.push(Entry { amount: $19.99 });
    let total: Decimal<2> = sum(entries);   // a copy — may leave the region
    post(entries);
}                                            // released here, O(1)
// `entries` is gone; `total` survives, because it was copied out
```

## 3. What this unblocks

Every ledger entry that was waiting on ownership:

- string concatenation, and interpolation producing a `String` value
- growable `List<T>`, and therefore a self-hosted compiler that is not capped
  at a fixed `[Node; 64]`
- string builders
- storing a `dyn` in a struct (though the real enabler was block scoping, not
  regions — see §6a)
- ~~returning a `dyn`~~ and ~~mutating methods through `dyn`~~ — **these were
  overclaimed.** A trait object borrows its source binding, a stack local, so
  neither was ever memory-blocked. Both need borrow/mutability tracking.

## 4. What M1 must NOT do

- **NO per-object borrow checking.** Ownership is region-granular. Adding
  object-level borrows is a separate, later decision — the whole point is that
  region granularity buys race freedom without it.
- **NO lifetimes in signatures.** If a signature needs a lifetime annotation to
  be checked, the design has drifted; find a region-level rule instead.
- **NO cross-region references.** A reference stays inside its region.
  *Trigger:* a required program genuinely needs a cross-region graph.
- **NO garbage collection or refcounting anywhere.** Region release is O(1) and
  deterministic. This is the predictability pillar; it is not negotiable.
- **NO implicit region.** Every heap allocation names, or is lexically inside,
  its region. A hidden global region would be a GC by another name.
- **NO region-in-region nesting** in the first cut. One level. *Trigger:* a
  required program needs sub-scopes.
- **NO thread transfer yet.** The single-owner rule is specified now so the
  design is compatible with it, but concurrency is its own milestone. Building
  ownership transfer before there are threads would be speculative.
- **NO `unsafe` escape hatch.** If it is needed, that is a signal the model is
  wrong, not a reason for a back door.

## 5. Deferred ledger

| Feature | Why deferred | Earns its place when |
|---|---|---|
| Cross-region references | Needs region lifetime relations | A required program needs a graph spanning regions |
| Nested regions | One level suffices to start | A required program needs sub-scopes |
| Thread transfer / shared regions | Concurrency is its own milestone | Threads exist |
| Per-object borrows | Region granularity is the bet | Region granularity provably blocks a required program |
| Region-allocated trait objects | Depends on §3 landing first | After `List<T>` works |

## 6. Implementation staging

Decision 3 was "implement fully" — but the risk that option itself named was a
long half-finished state. So it ships in slices, each one green and committed
before the next begins:

1. **`region` blocks + a bump allocator.** Open a region, allocate, release as
   a unit. No collections yet; a test proves memory is reused across regions.
2. **`List<T>`**, region-allocated and growable. This is the slice that
   unblocks the self-hosted compiler.
3. **Escape checking.** Refuse returning or storing region data beyond the
   region, with an error naming the region. This is the correctness core.
4. **String building** — concatenation and interpolation-as-a-value, which
   were the oldest entries on the ledger.
5. **Region-allocated `dyn`**, retiring the last two ledger entries.

Concurrency (ownership transfer) is explicitly NOT in this milestone; §2
specifies the single-owner rule only so nothing built here forecloses it.

## 6a. Staging corrections, found by building slice 1

Two things the staging in §6 got wrong, discovered immediately:

**`List<T>` as written needs generics, which Burxt does not have** (A6.0's
must-NOT list defers them explicitly). So slice 2 is **built-in growable
arrays** — a dynamic `[T]` alongside the existing fixed `[T; N]`, with the
element type known from the annotation — not a generic library type. Go's
slices are built in for the same reason. Generics remain their own milestone,
and `List<T>` can become a library type once they exist.

**Escape checking cannot come after the first allocation.** §6 listed it as
slice 3, but a region-allocated value that escapes its region is a
use-after-free — precisely the silently-wrong behaviour Burxt refuses
everywhere. So **escape checking ships WITH slice 2**, in the same commit as
the first thing that allocates. Shipping allocation without it would be
unsound, and "we'll add the check next" is not a standard this project applies
to anything else.

Revised staging:

1. **`region` blocks + bump allocator** — done. Mark the cursor, run the body,
   reset in O(1). No collector, no refcounts, no scheduler.
2. **Growable arrays + escape checking, together** — the slice that uncaps the
   self-hosted compiler, and the one that keeps it sound.
3. **String building** — concatenation. *Correction found while building it:*
   **interpolation-as-a-value is not actually memory-blocked.** It needs a
   number-to-string formatter writing into memory (Int and Decimal both), which
   is new machinery rather than an ownership question. Reclassified: concat
   ships here; interpolation-as-a-value moves to its own small slice once a
   formatter exists, and is no longer an M1 ledger entry.
4. **Storable `dyn`** — done, but NOT as predicted. A struct field may hold a
   trait object (block scoping already bounded it). The other two entries are
   re-diagnosed rather than retired: returning a `dyn` needs borrow tracking,
   and mutating through one needs mutability tracking. **A `dyn` borrows its
   source binding, which is a stack local — so regions were never the blocker
   for either.** §3 overclaimed; this is the correction.

## 7. Consequences to record elsewhere

- `FAR-HORIZON-ROADMAP.md`'s ARC lean is **superseded**. ARC was rejected for
  the reason surfaced earlier: it cannot deliver data-race freedom, which is
  now a must-have.
- `DESIGN.md`'s "data races as compile errors" moves from **ASPIRATION** to
  **COMMITTED**, with region ownership as the mechanism.
- `NOVELTY.md` §4 (effect handlers) stays compatible: effects capture state
  across suspension, and a region is a natural home for that state — a
  suspended computation holds its region.
- The "no runtime baggage" pillar holds without reinterpretation. A bump
  allocator is not a runtime; there is no collector, no scheduler, and no
  refcount traffic.

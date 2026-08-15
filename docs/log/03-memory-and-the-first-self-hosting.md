---
layout: doc
title: Memory, regions, and the first self-hosted pieces
section: log
description: *Milestone log, v0.0.21 – v0.0.30. The design these versions serve is in DESIGN.md; the whole log is indexed here.*
---

# Memory, regions, and the first self-hosted pieces

*Milestone log, v0.0.21 – v0.0.30. The design these versions serve is in [DESIGN.md](../../DESIGN.md); the whole log is indexed [here](README.md).*

The lexer and parser rewritten in Burxt — which disproved a claim that they were blocked on the memory model — then regions themselves in four slices, guaranteed tail calls, and exactness that survives the C boundary.

### v0.0.21: string bytes, and the first self-hosted piece

```text
print(byte_at("AbZ", 1));    // 98
```

`byte_at(s, i)` reads the i-th byte as an Int, bounds-checked with a message
naming bytes. It is **named for bytes deliberately**: A4.4 refused a bare
`s[i]` precisely because it would hide whether an index means a byte or a
character, and a builtin whose name says "byte" cannot hide it. Bytes
zero-extend, so a UTF-8 continuation byte comes back as 195, never negative.

**A Burxt lexer now exists, written in Burxt** — `examples/lexer.bx`, and a
test pins its output. It tokenizes real Burxt-ish source into `Plus`, `Number`,
`Name`, and friends, and it needs **no heap at all**:

- a token referring to source text carries a `(start, length)` **span**, not an
  owned substring — so no allocation;
- numeric literals are **accumulated arithmetically** as digits arrive
  (`value * 10 + (byte - 48)`), so no string building.

That is why the lexer runs before the memory model exists, and it is the
concrete first step of the self-hosting path. `match` earns its keep here: add
a variant to `Token` and the printer stops compiling until the case is handled.

**One compiler bug this found**, which is the value of writing real programs:
a struct field holding an enum panicked the compiler, because struct bodies
were filled in before enum types existed. Enums are now created first — a
total order, not a guess, since enum payloads are scalars and so can never
reference a struct. That fix is what lets the lexer return "the token, and
where to continue" as one `Scan` value.

### v0.0.22: the parser self-hosts — and the memory model was not the blocker

**`examples/parser.bx` is a Burxt expression parser and evaluator, written in
Burxt.** `1.00 + 2.00 * 3.00 = 7.00`, with correct precedence, parentheses, and
exact decimals — every result checked against an independent exact-decimal
implementation.

**The correction that matters:** v0.0.20 recorded that the parser was
M1-blocked, because an AST node is a recursive enum. That was wrong, and it was
wrong in an instructive way. **An AST does not need recursive types.** Nodes
live in a flat **arena** and refer to their children by **index**, which is how
Zig and Carbon build theirs. No recursion in the type, no heap, no memory
model. The parser was blocked on believing it was blocked.

What it actually needed were three restrictions lifted, none of them semantic —
all three were conservatism written early, not consequences of the design:

- **Arrays may hold aggregates.** A `[Node; 64]` is stack-allocatable; the old
  "elements must be Int, Bool or Decimal" was arbitrary. Nested arrays stay
  refused, with a reason: `a[i][j]` could not be written.
- **Structs may hold arrays.** The restriction's own message said "coming with
  the aggregate ABI" — which shipped in v0.0.12, so it was simply stale.
- **Indexing applies to any place, not just a bare name.** `self.nodes[i]`
  now reads and writes, via one `gen_place_addr` walker shared by both. This
  replaced a half-feature: an indexed *write* through a field path had briefly
  existed with no matching *read*.

Crucially, **no new semantics were added.** The arena mutates through a
`mut self` method — the by-reference receiver from v0.0.13 — so value semantics
stand untouched. Mutable aggregate *parameters* would have been a second
exception to A4.5's value-copy principle, and were deliberately not added.

**What self-hosting still needs from M1:** growable storage. The arena is a
fixed `[Node; 64]`, so a real compiler needs either a larger fixed budget or
heap growth. That is a genuine M1 dependency — but it is now a question of
*scale*, not of *expressibility*, which is a far smaller wall than the one
recorded two versions ago.

### v0.0.23: regions — M1 slice 1

```text
region tx {
    let inner: Int = outer + 1;
    print(inner);
}   // everything allocated here released in O(1)
```

The first slice of the memory model decided in `spec/1.0/M1-MEMORY-MODEL.md`:
**regions as the unit of ownership.** Opening a region records where the bump
cursor stands; closing it resets the cursor. That reset *is* the deallocation —
no per-object free, no refcount, no collector, no scheduler. The
no-runtime-baggage pillar holds without reinterpretation, because a pointer
that moves forward is not a runtime.

Region memory exhaustion is a named runtime error, not a silent overrun,
holding the same standard as every other check.

Refused with reasons, per the spec's must-NOT list: nested regions (one level
for now), and a region whose name collides with a variable.

**Two staging corrections the build immediately exposed**, both recorded in the
spec rather than worked around:

- **`List<T>` as specified needs generics, which Burxt deliberately does not
  have.** So the next slice is **built-in growable arrays** — a dynamic `[T]`
  beside the fixed `[T; N]`, element type from the annotation — not a generic
  library type. Go's slices are built in for exactly this reason.
- **Escape checking cannot come after the first allocation.** The spec had it
  as a later slice, but a region-allocated value that escapes is a
  use-after-free — the silently-wrong behaviour Burxt refuses everywhere. So it
  ships in the *same commit* as the first thing that allocates. "We will add
  the check next" is not a standard this project applies to anything else.

### v0.0.24: growable arrays + escape checking — M1 slice 2

```text
region parse {
    let mut nodes: [Node] = [];
    push(nodes, n);          // grows in the region
    print(len(nodes));
}                            // all of it released in O(1)
```

`[T]` is a growable array living in the enclosing region — distinct from the
fixed, stack-resident `[T; N]`. **No generics involved:** the element type comes
from the annotation, exactly as Go's slices are built in rather than generic.
Represented as `{ data, len, cap }`; `push` doubles capacity in the region when
full; indexing bounds-checks against the RUNTIME length.

**Escape checking ships in the same commit**, because allocation without it
would be a use-after-free. Two rules turn out to be sufficient:

1. **A region-allocated value may only be bound inside a region.** Since block
   bindings already do not escape their block, this single rule removes every
   assignment route out — there is nowhere outside to put it.
2. **A function may not return a region-allocated type.** That is the only other
   way the value could outlive the region its caller opened.

Taint propagates: a struct with a `[T]` field is itself region-allocated, so
`Holder { xs: [] }` outside a region is refused too. Both rules name the fix.

**The arena pattern self-hosting needs now works**: a struct holding growable
storage, mutated through a `mut self` method, inside one region — verified at
500 nodes, where the parser was previously capped at a fixed 64.

**The arena tradeoff, paid visibly:** growing copies into a fresh block and
abandons the old one, because a bump allocator cannot free an individual
object. That space returns when the region ends. Documented in the codegen
rather than hidden, since it is a real cost of the model.

### v0.0.25: string concatenation — M1 slice 3

```text
region r {
    let greeting: String = "Hello, " + name + "!";
    print(len(greeting));
}
```

`+` on String joins into the enclosing region, retiring the oldest entry on the
ownership ledger. The result is NUL-terminated, so a joined string is still a
plain `const char*` at the FFI boundary — indistinguishable from a literal, and
a test passes one to C's `strlen` to prove it. Byte equality works across the
two, since `==` was always about bytes rather than pointers.

Escape checking needed one addition, and the reason is worth recording: a
concatenated String lives in a region while a literal lives in `.rodata`, and
**both have type `String`** — so the type alone cannot say whether a value
escapes. The check therefore inspects the *expression*: `expr_allocates` walks
the tree, and returning anything it flags is refused.

**A reclassification found while building this:** interpolation-as-a-value was
recorded as M1-blocked, but it is not. It needs a number-to-string formatter
writing into memory — new machinery, not an ownership question. It is no longer
an M1 ledger entry; it becomes its own small slice once a formatter exists.

### v0.0.26: storable trait objects — M1 slice 4, and a corrected claim

```text
struct Holder { item: dyn Priced, label: Int }
let h: Holder = Holder { item: book, label: 1 };   // previously refused
print(h.item.price());
```

**A struct field may now hold a trait object.** The old refusal said a struct
"may outlive" what the object borrows — but when both are scoped to the same
block, it cannot. Block scoping was already doing the work; the refusal was
broader than the reason behind it.

This also fixed a real gap: **the concrete-to-`dyn` coercion only happened in
`let`**, so `Holder { item: book }` failed even though the equivalent binding
worked. The coercion now lives in `check_expr`, where every site that knows its
expected type passes through — struct fields, call arguments, returns — instead
of being special-cased in one place.

**A claim I got wrong, corrected here.** The M1 spec listed returnable and
storable `dyn` as things regions would unblock. Storable: yes. **Returnable:
no, and regions were never going to help.** A `dyn` borrows its *source
binding*, which is an ordinary stack local — so returning one dangles whether
or not a region is involved. Regions bound the lifetime of *region-allocated*
data; they do not change what a trait object points at. I briefly marked `dyn`
as region-allocated to force it, which broke every existing `dyn` test and was
the right kind of failure: the tests caught a category error.

So the remaining two ledger entries are re-diagnosed rather than retired:

- **Returning a `dyn`** — needs borrow tracking, not memory. Regions cannot fix
  it.
- **Mutating methods through a `dyn`** — needs to know the value behind the
  object was declared mutable. Regions bound its *lifetime*, not its
  *mutability*. The error now says exactly that.

### v0.0.27: the self-hosted parser is uncapped — M1 complete

```text
region parse {
    let mut a: Arena = Arena { nodes: [], count: 0, pos: 0, last: -1 };
    let root: Int = a.expr(src);
    print("{src} = {a.eval(root)}");
}
```

`examples/parser.bx` now uses `[Node]` instead of `[Node; 64]`, so **no node
budget is declared anywhere.** Verified on a 300-term expression: 599 nodes, all
allocated in one region and released together. That is what the memory model was
for.

**A link-time bug this found**, worth recording because it is a repeat of a
class already seen: two helpers each declared libc `fprintf`, so LLVM renamed
the second and the program failed to link against `fprintf.4`. Same collision
class as the reserved `main`/`stderr` symbols. There is now a single
get-or-declare helper, which is the general fix rather than a patch — any
runtime symbol declared in more than one place will do this.

**M1 is complete.** All four slices shipped: regions with a bump allocator,
growable arrays with escape checking, string concatenation, and storable trait
objects. Two of the spec's predictions were corrected along the way rather than
forced to come true (interpolation-as-a-value was never memory-blocked;
returnable `dyn` was never going to be fixed by regions), and both corrections
are recorded in the spec they came from.

### v0.0.28: reading a file, and rendering a value

```text
region source {
    let text: String = read_file("examples/sample.bx");
    print("--- {len(text)} bytes read");
    let n: Int = tokenize(text);
}
```

Two builtins, chosen because they were the two things a Burxt-hosted compiler
literally could not do: **it could not read its own input, and it could not build
an error message.**

- **`read_file(path) -> String`** reads a whole file into the current region,
  NUL-terminated, so it is an ordinary Burxt String afterwards. A file that
  cannot be opened is a *named* runtime error, not a silent empty string — the
  same standard bounds checks and overflow already meet. Why a builtin rather
  than FFI: `extern fn` returns are Int/CInt only, because a C function that
  returns a pointer returns memory belonging to nobody. `read_file` allocates in
  a region the compiler can see, so ownership stays answerable.
- **`to_string(v) -> String`** renders Int, Bool and Decimal into region storage
  using the *same format strings the printer uses* — one formatter, so a printed
  value and a rendered one can never disagree. `Bool` allocates nothing (both
  spellings are literals) and therefore needs no region. `to_string` on a String
  is refused: it would only copy it.

**And that retired the oldest entry on the ledger.** Interpolation-as-a-value
was reclassified at v0.0.25 as needing a formatter rather than memory. The
formatter now exists, so `let s: String = "n is {n}"` compiles — and it
**desugars to `to_string` + `+`** rather than getting a lowering of its own. A
test asserts the interpolation is byte-equal to the hand-written join, which is
the property the desugaring buys: they are the same program by construction.
`print("...{x}")` keeps its no-allocation path and still needs no region, so
nothing that used to compile got slower or stricter.

Escape checking needed no new rule — `expr_allocates` already flagged `+` on
Strings, so an interpolated value cannot outlive its region for the same reason
a concatenated one cannot.

**A repo hygiene fix shipped here too:** eight compiled example/test
executables had been committed. They are untracked now, with `.gitignore`
covering the bare, extensionless outputs `burxt build` writes into the working
directory.

### v0.0.29: guaranteed tail calls, and two region bugs found on the way

```text
fn count_down(n: Int, acc: Int) -> Int {
    if n <= 0 { return acc; }
    return tail count_down(n - 1, acc + 1);   // constant stack, or it will not compile
}
print(count_down(50000000, 0));               // 50 million frames deep
```

**`return tail f(...)` is a checked guarantee, not an optimization.** It lowers
to LLVM `musttail`, which *fails the build* if the call is not genuinely in tail
position — so there is never a silent difference between "optimized" and "hoped
for". The same program without `tail` dies at that depth, and a test asserts the
IR contains exactly one `musttail`, on the call that asked for it and nowhere
else. The guarantee is explicit by design: inferring it would mean a small edit
could silently reintroduce stack growth, which is the failure mode the whole
feature exists to remove. This is NOVELTY §4, and the same shape as every other
promise in the language — declare the intent, and the compiler guarantees it or
refuses with a reason.

`musttail` is only legal when the caller's and callee's **prototypes match**, so
that condition is checked in Burxt's own words rather than surfaced as an LLVM
verifier message:

```text
a guaranteed tail call reuses this frame, so `step` and `helper` must have the
SAME signature — `step` takes (Int) -> Int, but `helper` takes (Int, Int) -> Int.
```

Self-recursion satisfies that trivially, and mutual recursion does when the
signatures agree — which covers the loop use case. Also refused, each with its
own reason: a tail call into an `extern fn` (the C side owns that ABI, and
Burxt's width conversion has to happen *after* the call returns), aggregates
passed or returned by hidden pointer, and `return tail` on something that is not
a call. `tail` is now a keyword, so a program that used it as a name gets a
compile error rather than a changed meaning — the v0.0.17 syntax-change law.

**One refusal is a soundness rule rather than a limitation:** `return tail`
cannot leave a `region`. A region is released on the way out, but a tail call
never comes back to do it — and the release would have to happen *before* the
call, while the arguments may still point into the region.

**And that question exposed two real bugs in regions, both fixed here:**

- **A `return` from inside a region leaked it.** The cursor was only rewound at
  the closing brace, so leaving early skipped the release and the bump pointer
  climbed for the life of the process. A function that returned from inside a
  region leaked its region *on every call*. Now `return` releases the region on
  the way out, computing the result first (the expression may still be reading
  region storage). The regression test calls such a function 30,000 times, which
  would otherwise die of region exhaustion.
- **The return-path prover did not know a region body can return.** It demanded
  a second `return` after the block and then called that statement unreachable —
  there was no way to write a function that returns from inside a region at all.
  Before the fix the combination emitted invalid IR; a region is a lexical scope,
  not a branch, so if its body returns on every path, so does the region.

Worth stating plainly: **the tail-call work is what surfaced both.** Asking
"what has to happen between the call and the `ret`?" is the same question as
"what has to happen between the last statement and the `ret`?", and the second
one had two wrong answers.

### v0.0.30: exactness that survives the boundary (NOVELTY §1, slice 1)

```text
extern fn record_cents(amount: Decimal<2> as scaled) -> Int;

print(record_cents($19.99));   // C receives 1999 — exact, by declaration
```

Until now a Decimal simply could not cross into C. That was safe, but **"Decimals
cannot cross" is a missing feature; "Decimals cross only through an encoding that
cannot lose them" is a guarantee.** This slice converts the first into the second,
and the difference is the whole point of NOVELTY §1: real financial defects
overwhelmingly live at boundaries, not in arithmetic, and every language guards
the arithmetic and then abandons the wire.

**`CDouble`, an FFI-only type that models C's `double` honestly** — the same move
`CInt` made for C's `int`. It exists so a lossy crossing can be *named*, and
therefore refused. Without a name for the foreign type, "a Decimal may not bind
to a float" is unspellable, so the guarantee cannot be checked; it is merely
absent. Burxt still has no float type of its own and this is not one.

- **`Decimal<S>` → `CDouble` is a compile error, always**, with no flag and no
  escape. The message names the concrete loss and both exact alternatives:
  *"a C `double` cannot hold Decimal<2> exactly — a value like 0.10 is not
  representable in binary floating point, so this crossing would silently change
  the amount."*
- **`Int` → `CDouble` is allowed but range-checked at runtime.** A double holds
  every integer up to 2^53 exactly and starts skipping them after that, so
  `|n| > 2^53` is a named error with exit 70. Handing C a different integer than
  the one written is the same class of defect as a silent rounding.
- **A `CDouble` return stays refused.** Burxt has no exact way to receive a real
  number, and inventing an inexact receiver to complete the matrix would
  contradict the thesis. The error says how to get the value exactly instead.

**The marshaller is declared on the SIGNATURE, not applied at the call site**, and
that choice is the load-bearing one. The obvious alternative —
`record(scaled_of(price))` with `record` taking an `Int` — is weaker in exactly
the way §1 is about: the scale is gone from the type, so a `Decimal<4>`'s
unscaled integer type-checks identically, and so does an unrelated `Int`. **The
scale is lost at the boundary, which is the defect, not the fix.** Declared on
the signature, the scale IS the contract: `Decimal<4>` where `Decimal<2> as
scaled` was declared is a compile error, and every call site is then correct by
construction.

No `as text` marshaller was added: `c_fn(to_string(amount))` already does it
exactly (v0.0.28), and a feature whose only contribution is a second spelling
earns no place. The `CDouble` error points there by name.

**Linker pass-through, because an `extern fn` is only half an FFI.** Arguments
after the source file now go to the system linker unchanged
(`burxt run pay.bx cside.o -lm`), so the C being declared can actually be linked.
Burxt delegates linking to system tools and owns only object emission — the
position the platform roadmap already took. This is what let the guarantee be
tested against hand-written C rather than described: a test asserts `$19.99`
arrives as `1999`, that 2^53 crosses unchanged, and that 2^53+1 dies with a named
error instead of quietly becoming its neighbour.

Spec: `spec/1.0/N1-BOUNDARY-EXACTNESS.md`, with its own must-NOT list — no implicit
Decimal↔double conversion ever, no float type in Burxt, no "close enough" mode on
the range check, and no serialization layer yet (there is no encoder to guard;
when one is built it inherits these rules).

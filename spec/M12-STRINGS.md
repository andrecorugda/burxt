# Burxt — a String with an O(1) length (M12)

> Status: **specified, implementing.** The trigger fired and was measured; see
> `spec/M9-PERFORMANCE.md` §3 for the numbers that earned this milestone.

## 0. The problem, measured

A Burxt String is a bare NUL-terminated pointer. So `len` is a `strlen`. So a bounds check is a
`strlen`. So **reading n bytes costs n²**, and a compiler reads bytes all day.

M9 fixed the catastrophic instance of this — 190 seconds to 1.2 — by calling libc's `strlen` under
its real name so LLVM would hoist it, and by running the optimiser at all. It also wrote down that
the *shape* was untouched: hoisting changes how often the scan happens, not that a length is a scan.

The remainder, measured at v0.0.117 on programs of nothing but statements:

| Statements | 1600 | 3200 | 6400 | 12800 |
|---|---|---|---|---|
| Time | 0.00 s | 0.02 s | 0.09 s | **0.39 s** |

**Four times per doubling, at 180 KB.** A 400 KB Burxt program — the size this compiler's own
source already is — pays about 1.6 seconds just to be read. M9 deferred this with the trigger "a
program over a megabyte"; that estimate counted only the lexer, and the cost is in the whole front
end.

## 1. The decision: the length lives WITH THE BYTES

```
                 ┌────────┬─────────────────────┐
   a String  ──▶ │ length │ b y t e s … \0      │
                 └────────┴─────────────────────┘
                 ptr-8      ptr
```

A String value is still **one pointer**, pointing at the first byte. The length sits in the eight
bytes immediately **before** it. `len` is one load. `byte_at` bounds-checks against a load.

### Why not a fat value

The obvious alternative is `{ pointer, length }` as a two-cell value — what Rust and Go do. It was
rejected, and the reason is specific to this compiler rather than a general preference:

**Stage-1's emitter rests on one invariant: every Burxt value is one i64.** It is stated in
`runtime_ir()`'s own comment — "one value type through the whole emitter, cast at the few places a
pointer is needed". A two-cell String breaks that everywhere at once: every expression yielding a
String yields two registers, every String parameter becomes two arguments, `cells_of(String)`
becomes 2 and every record containing one changes layout. That is not a milestone, it is a rewrite
of the emitter, and it would be undertaken to gain a *register read* over a *cached load* — which
is not the difference between O(n²) and O(n).

A header keeps the invariant and still removes the quadratic. **The expensive property was O(1),
not zero-load.**

### The second reason, which is about correctness rather than cost

With a header, **the length cannot fall out of step with the bytes.** It is not a second field
travelling beside a reference that someone might copy, slice or reassign independently; it is part
of the object. A fat value has two things to keep true and one of them can be stale. This is the
same argument the language already makes for a record being its fields rather than a pointer to
them, and for a Decimal being an integer and a scale rather than a boxed pair.

### What it costs, said plainly

- **Eight bytes per String**, in the region. A program that builds many small Strings pays for it.
  The region is a bump pointer, so the cost is arithmetic, not fragmentation.
- **One load per `len`** rather than zero. Free next to a scan.
- **A String must always have a header**, which is a real constraint on where Strings come from —
  see §3.

## 2. What changes, and what deliberately does not

| | Before | After |
|---|---|---|
| A String value | one pointer to bytes | **unchanged** — one pointer to bytes |
| `cells_of(String)` | 1 | **unchanged** — 1 |
| Function ABI for a String | one i64 | **unchanged** |
| A record or enum holding a String | one cell | **unchanged** |
| `len(s)` | `strlen` — O(n) | one load — **O(1)** |
| `byte_at(s, i)`'s bound | `strlen` — O(n) | one load — **O(1)** |
| A String passed to C | a valid `char*` | **unchanged** — still NUL-terminated |
| A `char*` received from C | used directly | **copied into a region, with a header** (§3) |

That "unchanged" column is the whole argument for this design. The invasive-sounding milestone
touches the places that MAKE Strings and the two that MEASURE them, and nothing that stores,
passes or returns them.

Every place that makes a String must reserve eight extra bytes and write the length:
`substring`, `concat` (`+`), `to_string`, `read_file`, `argument`, interpolation, and every string
literal. In both compilers, and in `runtime_ir()`.

## 3. The C boundary, which is the part that is genuinely new

C has no header. So the two directions are not symmetric, and pretending they are is how this
milestone would introduce a memory bug.

**Burxt → C is free.** The pointer already points at NUL-terminated bytes. C never looks behind it.
An `external function` taking a `String` needs no marshalling at all.

**C → Burxt must copy.** A `char*` from `getenv`, `fgets` or any other C function has no header,
and reading `ptr[-8]` would read whatever happens to precede it — a silent wrong length, which is
worse than a crash. So a foreign string is **copied into the current region with a header**, and
that copy needs one `strlen`, at the boundary, once.

This is the honest accounting: the strlen does not disappear, it becomes **one per foreign string**
instead of one per byte read. And it becomes visible at the boundary, where a reader can see it,
rather than hidden inside `len`.

It also does not widen the pointer wall that `docs/guide/06-ffi.md` documents. Burxt still refuses
to receive a bare pointer as a `String` without going through the marshaller. If that page's rules
change, it changes in the same commit.

## 4. Acceptance

1. `len` and `byte_at` are O(1) in both compilers.
2. **The bounds check stays.** M9's rule: the fix was to compute the bound once, not to stop
   checking it. `byte_at` still refuses to read a byte the program does not own, and the fail
   fixture that proves it still passes.
3. The statement-count table in §0 becomes roughly **2× per doubling**. If it does not, this
   milestone did not do what it claims and the spec says so rather than the commit message.
4. `the_compiler_compiles_itself_without_going_quadratic` — the declaration ratio bar comes down
   from 25× to ~6×, in the commit that earns it. That comment already says so.
5. The byte-identical fixpoint holds, and the backend equality stays at all-of-them.
6. Peak RSS is re-measured. It may go **up** — eight bytes per String is real — and that belongs in
   the table, not a footnote. The 400 MB ceiling test is the backstop.
7. `examples/ffi.bx` and its `.c` file still work, and every `external function` in `lib/`.
8. A fixture that builds a String, measures it, slices it, concatenates it, and reads its bytes —
   because a length that is written in one place and read in another is exactly the kind of thing
   that works for the case you tested.

## 5. What this must NOT do

- **NO fat value.** §1. It breaks the emitter's one-value invariant for a register read.
- **NO removing the bounds check to go faster.** The whole point is that the bound is now cheap.
- **NO reading `ptr[-8]` on a foreign pointer.** §3. That is a silent wrong answer, and the reason
  the C direction is asymmetric.
- **NO abandoning NUL termination.** C interop is a shipped feature; the header is *additional*
  information, not a replacement representation.
- **NO guessing.** §0 has the before-numbers. Acceptance 3 and 6 need the after ones.

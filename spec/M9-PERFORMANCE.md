# Burxt — the compiler's own performance (M9)

> Status: **DONE (v0.0.90).** The self-compile went from **190 seconds to 1.17**, and peak
> memory from very nearly the 1 GB region limit to **196 MB**. The fixpoint still holds byte
> for byte, and all 27 invariants pass.
>
> Original status: **measured, to fix.** Not a feature: a wall. The Burxt compiler took three
> minutes on its own source and came within a hair of exhausting its 1 GB region, so the
> language could not grow further until this changed. Sixty lines of new code was enough to tip
> it over in v0.0.87, which is what turned a note into a milestone.

## 0. The result

| | Before (v0.0.89) | After (v0.0.90) |
|---|---|---|
| `stage1 examples/stage1.bx out.ll` | **190 s** | **1.17 s** |
| Peak RSS for that run | ~1 GB (region exhaustion imminent) | **196 MB** |
| 800 generated functions (175 KB) | 446 s | 0.97 s |
| 133 KB of comments and one statement | 36 s | 0.014 s |
| `cargo test`, all 27 invariants | — | 40 s |

## 0a. The numbers since, and the prediction coming true (v0.0.110)

Re-measured after generics landed, because a figure recorded once and never checked is a figure
nobody knows the truth of:

| | v0.0.90 | v0.0.110 | ratio |
|---|---|---|---|
| The compiler's own source | 283 KB | 365 KB | **1.29×** |
| `stage1 examples/stage1.bx` | 1.17 s | 1.96 s | **1.67×** |
| Peak RSS for that run | 196 MB | 239 MB | **1.22×** |

**Memory grew slightly LESS than the source did** — linear, which is what §3's note about a
linear working set predicts and what the region model should deliver.

**Time grew faster than the source, by a factor of 1.29** — and §3 named the cause in advance:
`find_fun`, `find_sym`, `find_type` and `find_method` each walk a growing array per lookup, so
the checker is O(n²) in declaration count. That was written down as "it does not bite yet: the
compiler has 40 functions". It has many more now, and the prediction is visible in the ratio.

It is not urgent — 1.96 s against a 20-second budget — and it is recorded rather than fixed for
the reason §5 gives about guessing: the fix is an index, Burxt has no map type, so it is a
feature and not a patch. **What changed is that there is now a test for the memory figure**, so
the next 20% cannot arrive unnoticed the way this one did. The 200 MB acceptance figure in §6.2
was true when written and is no longer; the ceiling the test enforces is 400 MB, which is a
guard against the 1 GB wall rather than a restatement of a past measurement.

## 1. The cause, and why it hid for eleven versions

**`byte_at(s, i)` bounds-checks against the string's length, and a Burxt String is
NUL-terminated, so the length is a `strlen`.** One `strlen` per byte read is O(n) per byte and
O(n²) per pass over a file — which is what a compiler does all day.

Two things kept it invisible:

1. **Stage-0 defined its own `burxt.strlen`** as a hand-written byte-at-a-time scan loop
   instead of calling libc's. LLVM cannot prove such a loop terminates, so the call was never
   `willreturn`, so **LICM refused to hoist it out of any loop**. Under its real name `strlen`
   is a recognised library function — known to read only its argument and always return — and
   LLVM hoists it unaided. It is also vectorised, reading a register at a time rather than a
   byte.
2. **Stage-0 never ran the mid-level IR pipeline at all.** `OptimizationLevel::Default` was
   set on the `TargetMachine`, which governs instruction selection and scheduling only;
   `write_to_file` shipped whatever `codegen.rs` built, unsimplified. There was no LICM to
   hoist anything.

Both are one-place fixes in `src/codegen.rs`. Together they took a plain byte loop over 133 KB
from **4.93 s to 0.001 s**.

**The check did not go away.** Every program still refuses to read a byte it does not own; the
length is computed once per loop instead of once per byte. That is the distinction worth
keeping: this was never a choice between correctness and speed.

Stage-1's emitter had called libc's `strlen` from the day it was written. So the compiler
Burxt wrote was right about this, and the compiler that wrote it was wrong — which is an
argument for having two.

### Three more of the same shape, in Burxt

Found by the same measurement and fixed in the same version:

- **`at_byte` in `types.bx`** measured `len(src)` on every probe, and `skip_trivia`'s comment
  loop re-walked the source per byte. The bound is now passed down from `run_range`, which is
  also more correct: interpolation lexes a *slice*, and a slice has no business reading the
  bytes after it. 36 s → 27 s on its own.
- **`end_of_line` and `starts_with_at` in `modules.bx`** did the same per byte of every header
  line.
- **`blank_imports` rebuilt the header a line at a time**, which is one copy of a growing
  String per line. A comment counts as header, every file in this compiler opens with a
  banner, and a file of nothing but comments spent 24 of its 29 seconds there. Only the `use`
  lines are rewritten now; everything else travels in whole spans. 27 s → 17 s.

## 2. How it was found — the method, not the luck

Three wrong guesses came first, and each one was cheap because it produced a number.

1. **v0.0.87 fixed the obvious quadratic** — a backwards linear scan for the type cache,
   replaced by an index. It bought 3 seconds out of 190.
2. **v0.0.89 counted the checker's own calls** and killed the leading hypothesis: `type_of`
   ran 18,369 times for 28,135 nodes — linear. Nothing retyped subtrees. No code changed, and
   it was the most valuable step so far, because it ruled out the fix that looked obvious.
3. **v0.0.90 stopped reading and ran a controlled experiment.** Take `gen200.bx` and pad it
   with 130 KB of comments: identical tokens, identical nodes, four times the bytes. 5.5 s →
   38.2 s. That one measurement said the cost was driven by `len(src)` and not by the program
   at all, and everything after it was bisection:
   - a file of *only* comments — one token — took 36 s, so it was not the checker
   - putting the code *first* so the module loader stopped at line 1 took it to 5 s, which
     isolated `modules.bx` from the lexer
   - a nine-line Burxt program that reads bytes in a loop was **also quadratic**, which moved
     the hunt out of the compiler and into the code stage-0 emits

**A note for whoever continues, learned the hard way.** The usual way to bisect a hot path —
stick an early `return` at the top of a suspect function and re-time — **does not work in
Burxt**: the compiler refuses unreachable code, so the probe will not build. Elimination here
means a nine-line standalone program, or a flag the function reads. Neither `perf` nor
`valgrind` was available either, and neither turned out to be necessary.

## 3. What is left, measured — and the one real fix for it

**The lexer is still quadratic, with a constant roughly 350× smaller.** Pure comments, timed
three times each and stable to a millisecond:

| Bytes | 189 KB | 378 KB | 756 KB | 1.5 MB |
|---|---|---|---|---|
| Time | 0.011 s | 0.038 s | 0.150 s | 0.650 s |

Still 4× per doubling. The arithmetic identifies it exactly: **one `strlen` per line**, not per
byte. 18,000 lines × 1.5 MB ÷ 32 bytes-per-cycle ≈ 1.7 G cycles ≈ 0.6 s, which is the number in
the table.

Why per line and not hoisted all the way out: `byte_at`'s check sits behind a short-circuit
(`i < stop && byte_at(src, i) != 10`), so it is **conditional**, so LLVM may not speculate it
above the loop — `strlen` on a pointer it cannot prove valid could fault. LICM lifts it out of
the inner byte loop and no further, which lands it once per iteration of the enclosing loop.

Reading the byte directly instead of through `at_byte`, where the bound is already known, was
tried and measured: **5%**. Recorded because it was measured, and because the guess behind it
(that `at_byte` was where the `strlen` came from) was wrong — it comes from the inner loops
themselves.

**The one fix that actually removes this: give a String an O(1) length.** Today a Burxt String
is a bare NUL-terminated pointer, so its length is a scan, so a bounds check is a scan, and no
amount of hoisting changes the shape — it only changes how often the scan happens. A String
carried as pointer-plus-length (still NUL-terminated, so C keeps working) makes `len` and
`byte_at` O(1) unconditionally, and stops the whole class of bug rather than the instances of
it. It touches both compilers, every `external function`, and `read_file`/`substring`/`concat`, so it
is a milestone and not a patch.

**Earns its place when:** a Burxt program processes a file large enough for it to bite — which
is over a megabyte on the numbers above, and is not the compiler's own 200 KB.

> **The trigger has fired (measured 2026-07-29, v0.0.117).** It did not need a megabyte, because
> the estimate above only counted the lexer and the cost is in the whole front end. Programs of
> nothing but statements, checked by stage-1:
>
> | Statements | 1600 | 3200 | 6400 | 12800 |
> |---|---|---|---|---|
> | Time | 0.00 s | 0.02 s | 0.09 s | **0.39 s** |
>
> **4× per doubling, at 180 KB.** Declarations behave the same way, and after v0.0.117 removed a
> separate quadratic in declaration COUNT the remaining 16× ratio for 4× the input is entirely
> this. So a 400 KB Burxt program — the size this compiler's own source already is — pays about
> 1.6 seconds to be read, and 800 KB pays six.
>
> That makes the pointer-plus-length String the next performance milestone rather than a deferred
> note. §3's reasoning stands unchanged; only the "not yet" does not.

**Also still linear scans, and smaller:** `find_fun`, `find_sym`, `find_type` and the parser's
`find_method` each walk a growing array per lookup, so the checker is O(n²) in declaration
count. On generated programs: 0.008 / 0.028 / 0.168 / 0.974 s for 100 / 200 / 400 / 800
functions, while RSS stays perfectly linear at 3.7 / 5.6 / 9.3 / 16.7 MB — CPU over a linear
working set is what a linear scan looks like. It does not bite yet: the compiler has 40
functions and 896 lookups over 40 entries is nothing. The fix is an index, and Burxt has no map
type, so that is a feature too.

## 4. Decisions taken along the way

**`write_bytes(path, buffer)` (v0.0.87).** A growable array already grows in amortised O(1),
so the missing piece was never a better String — it was a way to write a buffer of bytes.
`push` fills it, `write_bytes` empties it.

**Element assignment on a growable array (v0.0.87).** `xs[i] = v` and `self.table[i] = v`.
Stage-1 had allowed this since it had arrays at all; stage-0 refused it, and that divergence
is what forced an indexed cache to be written as a linear search.

**The optimiser runs (v0.0.90).** `default<O2>` over the module before the object is written.
An unoptimised native compiler is not really offering native output, and this is the pipeline
every other decision here assumed was already there.

## 5. What this must NOT do

- **NO abandoning the region model.** The allocator was never the problem; a collector would
  have hidden this rather than fixed it, and the region figure fell by 5× without touching it.
- **NO raising the 1 GB reservation to paper over it.** The pages are touched, so the memory
  is real, and a limit raised once is a limit raised again.
- **NO removing a bounds check to go faster.** The fix was to compute the bound once, not to
  stop checking it. `byte_at` still refuses to read a byte the program does not own.
- **NO optimising by guessing.** Every change here needed a before-and-after number. Three of
  the four guesses were wrong, and the numbers are the only reason that was cheap.

## 6. Acceptance

0. **A number for every attempt.** ✅ — v0.0.87 bought 1.5%, v0.0.89 changed no code and
   eliminated the leading hypothesis, v0.0.90 found the cause with a padded file.
1. A self-compile in **under 20 seconds**. ✅ **1.17 s.**
2. Region use during a self-compile **under 200 MB**. ✅ **196 MB.**
3. The fixpoint still holds, byte for byte. ✅
4. The suite's wall-clock time drops with it. ✅ 40 s for all 27 invariants.
5. Each step records its number. ✅ §2.
6. **A test that keeps it.** ✅ `the_compiler_compiles_itself_without_going_quadratic` — a
   file padded with comments must cost about what the unpadded one costs, and the self-compile
   must finish inside the budget §6.1 names. A ratio, so it does not depend on the machine.

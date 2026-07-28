# Burxt — the compiler's own performance (M9)

> Status: **measured, to fix.** Not a feature: a wall. The Burxt compiler takes **three
> minutes** on its own source and comes within a hair of exhausting its 1 GB region, so the
> language cannot grow further until this changes. Sixty lines of new code was enough to tip
> it over in v0.0.87, which is what turned a note into a milestone.

## 0. The measurement

`stage1 examples/stage1.bx out.ll`, on 46,493 tokens and 28,241 AST nodes:

| Phase | Time |
|---|---|
| Lex | 0.05 s |
| Parse | 0.05 s |
| **Typecheck** | **73 s** |
| **Emit** | **33 s**, and ~1 GB of region traffic |

Lexing and parsing are linear and fast. Everything after them is superlinear, and the region
figure is the more dangerous of the two: a program that exhausts its region **stops**, so this
is a correctness cliff rather than a slow build.

## 1. What is known, and what is not

**Fixed already, and it was not enough:** the type cache was a backwards linear scan over a
list that grew with every typed expression. It is now indexed by node (v0.0.87) — O(1) per
lookup, and the total time barely moved. Worth recording as a lesson: the obvious quadratic
was not the expensive one.

**Eliminated by measurement (v0.0.89).** The compiler now counts its own calls and reports
them:

```
parsed 178 items and statements into 28,135 nodes
work: type_of called 18,369 times for 28,135 nodes
```

**`type_of` is linear.** Nothing retypes subtrees, so the leading hypothesis — exponential
work in expression depth — is wrong. 73 seconds over 18,369 calls is **≈4 ms per call**, which
means the cost is *inside* one call rather than in how often it is made. That narrows the hunt
sharply and rules out the fix that looked obvious.

**A note for whoever continues, learned the hard way.** The usual way to bisect a hot path —
stick an early `return` at the top of a suspect function and re-time — **does not work in
Burxt**: the compiler refuses unreachable code, so the probe will not build. Elimination here
means deleting a body, or adding a flag the function reads. That is the language's strictness
biting its own author, and it is worth knowing before an afternoon is spent on it.

**Still unidentified**, in the order to check:

1. ~~Repeated `type_of` on the same node~~ — **ruled out**, see above.
2. **`ty_show` and message building on paths that are not errors.** A String built and
   discarded per node would explain both the time and the region traffic.
3. **`find_sym` / `find_fun` / `find_type` linear scans.** Each is small per call, and every
   one is called per node.
4. **`expr_allocates` re-walking subtrees**, once per `return` and once per aggregate field.
5. **String concatenation still on hot paths**, which has cost this project four versions
   already and is the reason `write_bytes` now exists.

## 2. Decisions already taken

**`write_bytes(path, buffer)` (v0.0.87).** A growable array already grows in amortised O(1),
so the missing piece was never a better String — it was a way to write a buffer of bytes.
`push` fills it, `write_bytes` empties it. Anyone producing large output needs exactly this,
which is the test a builtin has to pass.

**Element assignment on a growable array (v0.0.87).** `xs[i] = v` and `self.table[i] = v`.
Stage-1 had allowed this since it had arrays at all; stage-0 refused it, and that divergence
is what forced an indexed cache to be written as a linear search. Both forms are bounds-checked
against the header's length at run time.

## 3. What this must NOT do

- **NO abandoning the region model.** The allocator is right; what is wrong is how much is
  allocated. A collector would hide the problem, not fix it.
- **NO raising the 1 GB reservation to paper over it.** The pages are touched, so the memory
  is real, and a limit raised once is a limit raised again.
- **NO caching that can go stale.** The type cache is indexed by node because a node's type is
  a fact about the node; anything keyed on something looser would be a bug waiting.
- **NO optimising by guessing.** Every change here needs a before-and-after number in the
  commit message. The first attempt in v0.0.87 fixed the obvious quadratic and moved the total
  by 3 seconds out of 190, which is exactly what guessing earns.

## 4. Acceptance

0. **A number for every attempt.** v0.0.87 fixed the obvious quadratic and bought 1.5%;
   v0.0.89 measured and eliminated the leading hypothesis without changing any code. Both are
   progress, and only the second kind is trustworthy.
1. A self-compile in **under 20 seconds**, measured the same way: `time stage1
   examples/stage1.bx out.ll`.
2. Region use during a self-compile **under 200 MB**, so the headroom is real rather than
   marginal.
3. The fixpoint still holds, byte for byte.
4. The suite's wall-clock time drops with it, since four of its tests self-compile.
5. Each step records its number, so the next person knows what was tried and what it bought.

# Burxt — "Vectors, Exactly" (N9)

> Status: **rows 1–5 are BUILT (v0.0.195).** `lib/vector.bx`, with the dimension contract,
> brute-force top-K, `vector_normalise`, the overflow wall as a panic fixture, and a JSONL store that
> survives a real file. Rows 6–9 remain, and every one of them needs the language to grow. Every number
> in §2 and §3 came from running the compiler, not from reading it.
>
> **One row of §3 looked blocked by the type system. It was not, twice over, and both answers are
> worth keeping.**
>
> `vector_normalise` was absent because dividing a `Decimal<7>` needs a rounding contract, so a
> normalised component is a `Decimal<7, RoundHalfEven>` — and `push` would not accept a plain
> `Decimal<7>` into a contracted array, so the contracted API could not be built either.
>
> **First answer (v0.0.194): the second half was a bug.** Seven positions in stage-0 still compared
> types with `==` while a comment on `storable` claimed the widening applied everywhere. `return` was
> one of them, and the comment's own list named `return`. Fixed at every site, in both compilers, and
> the rule is now checked by `a_contract_widens_at_every_position_and_drops_at_none` rather than
> asserted in prose — a comment cannot be run.
>
> **Second answer (v0.0.195): the contract was never needed.** With the bug fixed the division-based
> version compiled, but it answered `[Decimal<7, RoundHalfEven>]`, which cannot be handed back to
> `vector_dot` — dropping a contract at a slice's element type is refused, and rightly, since a slice
> is aliasable and the callee could write a rounded value back. That would have split the API in two.
> Asking `(tick*n) * mag <= |c|` instead of `tick*n <= |c|/mag` removes the division entirely, so the
> answer is a plain `[Decimal<7>]` that composes with everything above it.
>
> **Worth recording as a pattern**, because this is the fifth and sixth time: a wall that looked like
> the type system holding a line was one rule applied inconsistently, and then a quotient that did not
> need to be formed. The questions to ask first are whether the rule applies here at all, whether it
> applies everywhere else too, and whether the operation the type system is objecting to is one the
> problem actually requires.
>
> This came out of the production-readiness audit
> ([FAR-HORIZON-ROADMAP.md](FAR-HORIZON-ROADMAP.md#5-two-decisions-this-audit-says-should-be-re-opened)),
> which was asked to say whether Burxt's no-float decision blocks Andre's RAG vision. The answer is
> the interesting kind: **it does not block it, it changes it into something no other store can offer.**

## 0. The claim

> **The same query returns byte-identical similarity scores on every CPU, every target, and every
> run — and the compiler stops rather than silently losing a digit.**

No float-based vector store can say either half.

Not because their engineers are careless. `f32` addition is **not associative**: `(a+b)+c` and
`a+(b+c)` differ in the last bits, so a dot product's answer depends on the order the SIMD lanes
happened to reduce in — which depends on the CPU, the compiler version, and the thread count. Every
production vector database therefore has scores that wobble in the last few digits between machines,
and nobody treats it as a bug because there is no alternative available to them.

Scaled-integer arithmetic **is** associative and exact. So the wobble is not reduced, it is *absent*.

### Why that matters beyond tidiness

- **A reproducible ranking.** Two runs, two machines, or a re-index cannot silently reorder results
  that were nearly tied. Today "the top result changed and nothing changed" is unfalsifiable.
- **A regression test for retrieval is possible.** You can assert a score, not a range. Nobody can do
  that now, which is why RAG quality is evaluated statistically rather than pinned.
- **An audit trail that means something.** "This answer was retrieved from these documents with these
  scores" is a claim you can re-verify a year later, on different hardware. That is the same argument
  as exact money, applied one layer up — and it is the reason this belongs in Burxt rather than
  being a library anyone could write.
- **The failure direction is right.** Precision loss is a *trap*, not a shrug. §2 shows the wall.

## 1. What the math needs, and what it does not

The reflex is "cosine similarity needs a square root, and a square root needs floats." Both halves of
that are avoidable, and the way round is worth stating because it is the whole design:

| Metric | Formula | Needs a square root? |
|---|---|---|
| **Dot / inner product** | `Σ aᵢbᵢ` | **No** |
| **Squared Euclidean** | `Σ (aᵢ-bᵢ)²` | **No** — and it ranks identically to the distance, because `√` is monotonic |
| Cosine | `Σ aᵢbᵢ / (‖a‖·‖b‖)` | Only to normalise — **once, at insert**, not per query |
| Euclidean | `√Σ (aᵢ-bᵢ)²` | Yes, and only to report a human-readable number |

So the two metrics that carry almost all real retrieval — inner product and squared L2 — need **no
square root at all**, and are exact today.

Cosine is exact too, on one condition: **store vectors normalised.** That is what production systems
do anyway, and the major embedding providers already return unit-length vectors — so in the common
case there is nothing to normalise and cosine *is* the dot product.

When normalisation is needed, it needs **no square root and no division** — which is not what this
spec originally predicted, and the correction is the interesting part.

The prediction was an integer square root plus a division with a rounding contract. What `vector_dot`
already showed is the better move: a comparison beats a quotient. Finding each component's scaled count
by binary search asks

```
tick*n  <=  |c| / mag        as        (tick*n) * mag  <=  |c|
```

and the right-hand form is two multiplications of `Decimal<7>` landing at scale 14, where `|c|` also
lands once lifted by `1.0000000`. Both sides exact, same type, no quotient, and the answer is a plain
`Decimal<7>` rather than a contracted one — so it composes with `vector_dot` instead of needing a twin
of every function.

**One measurement changed the design, and it is worth stating because the naive version looks fine.**
Dividing by `vector_magnitude(a)` directly is wrong by **4.1e-5** on a short vector — 400× worse than
the scale suggests — because the magnitude is floored to seven places, so its 1e-7 *absolute* error
becomes *relative* to a small magnitude. The fix costs no precision at all: **scaling a vector by ten
does not change its direction, and is exact.** Boost until the magnitude is at least 1, measure it
there, and divide boosted by boosted. Worst case over 399 vectors spanning three orders of magnitude:

| | worst magnitude | reading exactly unit |
|---|---|---|
| dividing by the floored magnitude | 1.0000411 | 75 of 399 |
| **boosting first** | **1.0000001** | **290 of 399** |

Normalisation is therefore a **stated rule** rather than a hidden approximation: *nearest seven-place
value, ties toward zero, applied to the size with the sign restored after* — so a component and its
negation round symmetrically, which matters because embedding components are half negative.

One consequence to know before it surprises you: `vector_is_unit` answers false on about a quarter of
freshly normalised vectors, and that is correct. It asks whether the magnitude reads exactly 1 at seven
places — a window one grid step wide — and rounding components onto that same grid moves the magnitude
by about one step. It is what "unit length on a finite grid" means, not a defect in either function.

## 2. The scale arithmetic — verified, not estimated

This is the part the type system does that nothing else does, and the numbers are exact.

A component of an embedding is in `[-1, 1]`. At scale `S` its unscaled integer is at most `10^S`. A
product of two lands at scale `2S` with magnitude up to `10^(2S)`. Summing `D` of them:

```
D × 10^(2S)  <  2^63 ≈ 9.22 × 10^18
```

| Scale | Product scale | Max dimensions | Verdict |
|---|---|---|---|
| `Decimal<6>` | `Decimal<12>` | ~9,200,000 | comfortable, 6 places of component precision |
| **`Decimal<7>`** | **`Decimal<14>`** | **~92,000** | **the sweet spot — covers every real embedding size** |
| `Decimal<8>` | `Decimal<16>` | ~920 | too tight: 1536 dims **overflows** |
| `Decimal<9>` | `Decimal<18>` | ~9 | unusable |

**Measured, at the worst case** — 1536 dimensions, every component `0.9999999`:

```burxt
function dot(a: [Decimal<7>], b: [Decimal<7>]) -> Decimal<14>
    requires len(a) == len(b)
{ ... }

dot(x, y)  →  1535.99969280001536        // exact, and the same everywhere
```

And at scale 8, the same 1536 dimensions:

```
burxt runtime error: arithmetic overflow — the exact result no longer fits in the value range
```

**It traps.** Not a wrap, not a saturate, not a quietly-wrong score. That is the property: a Burxt
vector store either answers exactly or refuses to answer, and there is no third outcome where it
answers approximately without telling you.

Note what the scale table also gives you: **`Decimal<7>` is more component precision than `f32`
carries in this range** (f32 has ~7 significant decimal digits total, and near 1.0 its spacing is
about 6×10⁻⁸). So exactness is not being bought with precision here — it is close to free.

## 3. What is needed, per piece

Ordered so that each row is useful on its own. **The math needs nothing new**; everything below it is
storage and ingestion.

| # | Piece | What it is | Needs from the language | Size |
|---|---|---|---|---|
| 1 | **`lib/vector.bx`** ✅ | **done (v0.0.192, `normalise` v0.0.195).** `dot`, `squared_distance`, `magnitude_squared`, `magnitude`, `is_unit`, `normalise`, `top_dot`. `Decimal<7>` in, `Decimal<14>` out, and no division anywhere | **nothing** | small |
| 2 | **A dimension contract** | `requires len(a) == len(b)`, and a stated max dimension per scale so the overflow is a compile-time-documented limit rather than a surprise | nothing — this is what `requires` is for | small |
| 3 | **Brute-force search** | scan N vectors, keep the top K. Exact, and correct by construction — the baseline every index is checked against | `lib/array.bx` for the top-K (partial sort) | small |
| 4 | **A text-format store** ✅ | **done (v0.0.193).** JSONL — one JSON object per line, via `lib/json.bx`. `vector_store_render`/`_parse` are pure; `_write`/`_read`/`_append` declare `touches files` | nothing. `lib/json.bx` existed | small |
| 5 | **Reproducibility test** | the same corpus and query, scored on two targets, asserting **byte-identical** output. This is the CLAIM, so it is the test that matters most | cross-compilation (#9) to be a real cross-target check; single-target is worth having first | small |
| 6 | **A binary store format** | fixed-width records, `mmap`-able, with a header and a checksum | **bitwise ops: DONE (v0.0.199)**, and CRC-32 is a fixture. Integer widths still absent, but bytes can be packed by hand — so this row is now buildable. `mmap` wants a length-taking `c_bytes_at` | medium |
| 7 | **Durability** | append-only log, `fsync`, crash recovery, atomic rename | `fsync` is a pointer-free syscall so it clears the wall; needs an `external` declaration and a `touches files` | medium |
| 8 | **Embedding ingestion** | call a model, get vectors back | **the pointer wall** (HTTPS), or the honest interim: read embeddings from a file another process wrote | blocked |
| 9 | **An ANN index** | HNSW or IVF, for corpora where brute force is too slow | needs #6, and a decision about whether an *approximate* index may be exact-scored — it may: approximate CANDIDATE selection, exact SCORING | large |
| 10 | **`touches model`** | the effect already exists in the language and has nothing to attach to. This is what it was reserved for | nothing | small |

### Row 4, as built

**JSONL, not one big array**, and the reason is what a store actually needs: a line can be appended
without rewriting the file, and a corrupt line costs one row rather than the whole corpus.

```
{"id":"east","values":["1.0000000","0.0000000","0.0000000"]}
{"id":"down","values":["-0.6000000","0.8000000","-1.0000000"]}
```

**Components cross as quoted digit strings.** This is `lib/json.bx`'s position applied one scale up, and
it is the whole reason the row is worth building rather than assuming: a JSON *number* reaches almost
every consumer as a double, so writing exact vectors and reading them back through a float would defeat
the point of the file above it. `component_from_json` **never rounds** — `"0.12345678"` answers `None`
rather than `0.1234568`, because a component arriving with more precision than the store holds is a
question, and the writer meant those digits.

Split in two on purpose: `vector_store_render`/`vector_store_parse` are pure and take no effect, so the
format is testable without a disk; `vector_store_write`/`_read`/`_append` declare `touches files`. There
is a fail fixture proving a caller cannot launder that effect
(`tests/fail/vector_store_needs_the_files_effect.bx`), which is what makes "load the corpus" visible in
a signature rather than discoverable by reading a body.

Covered by `tests/pass/vector_store.bx` (the format, and what it refuses) and
`the_vector_store_round_trips_a_file` in `tests/runner.rs` (a real file, an append, and the scores
asserted as **numbers** on the way back out — which is row 5's claim applied to persistence, and the
half that matters in practice, because a store that drops a digit on the way to disk has exactly the
wobble the arithmetic was built to remove).

### The interesting shape of that table

**Rows 1–5 are buildable today, need no language change, and deliver the claim.** A slow, exact,
reproducible vector store with a JSON-file backend is a working demonstration of something nobody
else has — and it is a week of work, not a year. **As of v0.0.195 all five are built**, and the count
of language changes it took was zero. One bug was found on the way (v0.0.194) and one prediction in §1
turned out wrong in the useful direction — normalisation needed neither the square root nor the
division this spec expected.

Rows 6–9 make it *fast*. Row 8 makes it *convenient*. None of them make it more correct.

That ordering is the opposite of how a vector database is usually built, and it is the right way round
here, because the differentiator is the arithmetic and not the index.

## 4. The rule that makes this Burxt rather than a library

Row 10 deserves its own note. `touches model` exists in the language and currently has nothing to
attach to — it was added for exactly this.

Once a model client exists, the rule from `NOVELTY.md` candidate 8 becomes enforceable and is worth
stating as the design's centre:

> **A function that produces money may not reach a model.**

An LLM may decide *what to do*. It may never decide *what a number is*. In a RAG system that is not a
philosophical position, it is the difference between "the model chose which invoices to summarise" and
"the model chose what the total was" — and the second is how an agent makes a costly mistake.

The effect machinery to enforce it is already complete: a closed effect set, declared not inferred,
transitive by declaration, and checked at every call including through methods since v0.0.183. What is
missing is the rule and a fail fixture per path.

## 5. What this does NOT claim

- **Not faster.** Scaled i64 arithmetic vectorises fine, but a tuned f32 SIMD kernel with an ANN index
  will beat brute-force exact scoring on a large corpus for a long time. The claim is reproducibility,
  not throughput.
- **Not better recall.** Exactness changes the *score*, not the *embedding*. If the model's vectors
  are poor, exact scores of poor vectors are still poor.
- **Not a reason to add floats.** The audit's count says floats block a narrower set than the pointer
  wall does, and this spec is the reason: the flagship use nobody thought was reachable without them
  turns out to be reachable, and better, without them.
- **Not blocking, and not urgent.** Bugs first, then usability. This is written down so it is not
  re-derived, not so it jumps a queue.

## 6. Acceptance

1. `lib/vector.bx`, with a fixture per function, in **both compilers** — the fixpoint holds.
2. The 1536-dimension worst case is a fixture, answering `1535.99969280001536` **exactly**.
3. The scale-8 overflow is a **panic fixture**. The wall is a feature and has to be tested as one.
4. ~~`isqrt`~~ — not needed. `magnitude_of_squared` binary-searches the scaled count and compares
   squares, so no integer square root appears in the library at all. Checked at 0.6/0.8 exactly, at
   √2, and against a 399-vector sweep asserting the magnitude of every normalised vector reads 1 to
   within ONE tick (`tests/pass/vector_normalise.bx`). The zero vector is a panic fixture.
5. Brute-force top-K over a small corpus, with the ranking asserted — not a range, an ordering.
6. **The reproducibility test**: the same corpus and query scored twice, byte-identical. On one target
   now; across targets when M3 lands.
7. `spec/NOVELTY.md` candidate 8 (money may not reach a model) has a fail fixture per path — direct,
   through a call, and through a method — once there is a model client to reach.

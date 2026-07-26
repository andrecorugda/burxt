# Burxt — Provably Reproducible Functions (NOVELTY §2, slice 1)

> Status: **specified, to implement.** The first slice of `NOVELTY.md` §2
> ("provably deterministic money math via forbidden effects"), which the register
> listed as *buildability: medium — needs an effect system first*. It needs less
> than that: one declared effect marker already exists (`allocates`, v0.0.38), and
> this is the same shape pointed the other way — a marker that **forbids** rather
> than permits.

## 0. The claim

> **This function's result depends only on its arguments. The compiler checked.**

Financial auditors and regulators care intensely whether a calculation is
reproducible, and today the honest answer in every language is *"we believe so."* A
hidden `DateTime.Now`, a locale-dependent parse, or a config lookup three calls down
silently makes a computation irreproducible, and nothing catches it.

Burxt already guarantees the *arithmetic* is exact and byte-identical across
targets. This extends that to the *inputs*: nothing may enter a calculation except
through its parameters.

```text
pure fn interest(balance: Decimal<2, RoundHalfEven>, rate: Decimal<4>)
    -> Decimal<2, RoundHalfEven>
{
    return balance * rate;
}
```

## 1. Decisions

### Decision 1 — `pure` is declared on the function, and checked

Same reasoning as `allocates`: a guarantee is written where it applies. Inference
would make purity an invisible property that a distant edit can silently revoke,
and the whole value here is that the guarantee is *stated* — an auditor can read the
signature.

### Decision 2 — what a `pure` function may not do

- **Print.** Output is an effect. `pure` means the function computes its result and
  does nothing else.
- **Read a file.** The result would depend on the filesystem.
- **Call into C** (`extern fn`). What the other side does is not something Burxt can
  promise anything about — and the FFI is where nondeterminism actually enters a
  Burxt program today.
- **Call a function that is not `pure`.** The guarantee cannot rest on a function
  that does not make it. This is what makes the property transitive without
  inferring anything.

### Decision 3 — what it may do, deliberately

Arithmetic, comparisons, `match`, loops, local `let mut`, field access, array and
string reads, and **allocation**. Allocation is deterministic: a bump allocator
returns the same layout for the same sequence of calls, and the region model means
it cannot observe anything about the outside world. `pure fn f(...) -> String
allocates` is therefore a legal and useful combination — a pure function that builds
a string.

### Decision 4 — purity constrains the callee, never the caller

Any function may call a `pure` one. Nothing propagates upward. That keeps the marker
free to adopt one function at a time.

### Decision 5 — honest about today's teeth

Burxt has **no clock, no random, no locale, no environment access and no ambient
configuration.** So the rules above bite on I/O and the FFI, and otherwise they are
a **forward guarantee**: when a clock is added, `pure` already forbids it, and it
will be added *behind* this rule rather than in front of it. Stating that plainly
matters more than overselling what is enforced on the day it ships.

## 2. What this must NOT do

- **NO inference of `pure`.** See Decision 1.
- **NO "mostly pure" or opt-out.** There is no annotation that suspends the check
  inside a `pure` function. If a calculation needs the outside world, it is not
  pure, and the fix is to pass the value in as a parameter — which is the entire
  point.
- **NO purity-based optimisation** in this slice. Common-subexpression elimination
  and memoisation are things the guarantee *enables*, and doing them now would mean
  the marker changes behaviour as well as legality. It must first only ever change
  what compiles.
- **NO `pure` on `extern fn`.** Burxt cannot check the other side.
- **NO clock, random, locale or environment access added anywhere** until it can be
  added behind this rule.

## 3. Deferred

| Feature | Why deferred | Earns its place when |
|---|---|---|
| `pure` methods | Methods need the marker on the receiver clause; a pure function therefore cannot call a method yet | A required program needs a pure method |
| `pure` as a *requirement* on a parameter (a callback that must be pure) | Needs function types, which do not exist | Function values exist |
| Purity-driven optimisation | Must not change behaviour in the slice that introduces the marker | The guarantee has been stable for a while |
| Effect *inference* (§6's handlers) | A different and much larger system | Effect handlers are specced |

## 4. Acceptance

1. `pure fn` compiles and computes; a pure function may call another pure function.
2. `pure fn` + `allocates` compiles and may build and return a String.
3. Printing inside a `pure` function is a compile error saying output is an effect.
4. Reading a file inside one is a compile error naming reproducibility.
5. Calling an `extern fn` from one is a compile error naming the C boundary.
6. Calling a non-`pure` function from one is a compile error naming both functions.
7. A non-pure function may freely call a pure one.
8. Calling a method from a `pure` function is refused with the reason recorded in
   §3 (methods cannot yet carry the marker), not with a confusing message.

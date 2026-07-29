---
title: Memory
---

# 4. Memory

No garbage collector. No reference counting. No borrow checker. One idea instead: **the
region**.

## Why anything is needed

Some values exist before the program runs — a literal, an `Int`, a record on the stack.
Others are *built* while it runs: joining two Strings, `to_string`, `substring`,
`read_file`, a growable array. Built values need storage, and something must decide when
that storage goes away.

C says "you decide, and good luck". Java and Go say "a collector will notice eventually".
Rust says "prove it with lifetimes". Burxt says: **write down where it lives.**

## Regions

```burxt
region r {
    let message: String = "line " + to_string(42);
    print(message);
}
// the storage is gone here
```

A region is a bump pointer and a mark. Opening one remembers where the pointer stood;
closing it puts the pointer back. That is the whole of release: **O(1), regardless of how
many allocations happened inside**, with no traversal, no finalizers and nothing to
schedule.

Outside a region, building anything is a compile error:

```
error: joining two Strings allocates, so it needs a region: there is none open here.
       Open one with `region r { ... }`, or declare the function `allocates` to build
       in the caller's region.
```

## `allocates` — building for your caller

A function body has no region of its own, which would make a helper that formats a message
impossible to write. `allocates` says: *I build in my caller's region.*

```burxt
function describe(line: Int) -> String allocates {
    return "line " + to_string(line) + ": unexpected byte";
}

region parse {
    print(describe(3));      // the bytes belong to `parse`
}
```

Every call to an `allocates` function needs a region open at the **call site**. The value
never outlives its region, so the rule is satisfied by construction — the compiler simply
had no way to know the function intended it, which is why it is written down.

It is one bit, not a lifetime: no names, no scopes, nothing to unify.

## What escapes, and what the compiler refuses

A built value may leave a function **only** when it was built in the caller's region — that
means `allocates`, and no region of the function's own open around the return.

```burxt
function bad() -> String {
    region r { return "a" + "b"; }
}
```

```
error: cannot return this: it was built inside a `region` block, which ends at its
       closing brace, so its storage would not outlive it
```

The escape check looks **inside aggregates**: a record holding a built String is itself
built, and an array of them likewise. That arm was missing for four versions and let a
use-after-free through — which is why it walks fields and elements now, and why the example
in [`examples/regions.bx`](../../examples/regions.bx) spells out every refusal.

## Regions do not nest

```
error: regions do not nest: the inner one would end while the outer is still open, and
       one bump pointer cannot serve two marks
```

## When you run out

The reservation is 1 GB per process, lazily mapped, so the cost is virtual until used.
Exhausting it is a named failure with exit 70, not a crash:

```
burxt runtime error: region memory exhausted — this build reserves 1 GB per process
```

An allocator that does not check its limit does not fail; it corrupts. That distinction cost
a debugging session in v0.0.73, and it is the reason the check exists in both compilers.

## Next

[Contracts](05-contracts.md) — claims the compiler checks.

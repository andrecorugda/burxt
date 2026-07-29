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

## Building for your caller — and why you no longer say so

A function body has no region of its own, which would make a helper that formats a message
impossible to write. So a function that builds a value builds it **in its caller's region**:

```burxt
function describe(line: Int) -> String {
    return "line " + to_string(line) + ": unexpected byte";
}

region parse {
    print(describe(3));      // the bytes belong to `parse`
}
```

Every call to a function that builds needs a region open at the **call site**. The value
never outlives that region, so the rule holds by construction.

### `allocates`, and why it is now optional

Until v0.0.142 `describe` had to be written `-> String allocates`, declaring what it did.
That word is no longer required, and the reason it went is worth knowing, because it says
something about how this language is meant to feel.

The compiler always computed the answer — it walks a body and works out whether it
allocates, then checked the declaration against what it found. So the programmer was being
asked to write down a fact the compiler had already derived. It was required for an ordering
reason, not a semantic one: a call site needs to know about a callee that might be declared
200 lines later, so the answer had to be available before any body was read.

What settled it was writing a real program. In [`examples/pos/receipt.bx`](../../examples/pos/receipt.bx)
the word was on **three functions out of three** — an annotation on everything, telling a
reader nothing.

Writing it still works, and is still checked:

```burxt
function describe(line: Int) -> String allocates {    // fine, and verified
```

It is still **required** on `external function`, where there is no body to look at.

The design record is [`spec/M14-IMPLICIT-REGIONS.md`](../../spec/M14-IMPLICIT-REGIONS.md),
which also covers what comes next: every `{ }` becoming a region, so the `region` block goes
the same way.

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

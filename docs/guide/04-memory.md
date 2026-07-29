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
Rust says "prove it with lifetimes". Burxt says: **release a whole batch at once, and let the
compiler work out which batch.**

## Think of a tray

Your program has a tray. Everything it builds goes on the tray. When you are finished with
everything on it, you tip the whole tray into the bin in one motion — you never pick items
off it one at a time.

```burxt
let message: String = "line " + to_string(42);
print(message);
```

That works with nothing else around it. **You do not have to write anything down.** A program
has a tray from the moment it starts, and the code above is a complete program.

## `region` — a second tray, for work you want off your hands early

```burxt
region r {
    let message: String = "line " + to_string(42);
    print(message);
}
// everything built inside is gone here
```

A region is a bump pointer and a mark. Opening one remembers where the pointer stood; closing
it puts the pointer back. That is the whole of release: **O(1), however many allocations
happened inside**, with no traversal, no finalizers and nothing to schedule.

### When you actually need one

Until v0.0.146 you needed one for everything: building anything outside a region was a compile
error, and every program opened with a `region` wrapper it never mentioned again. That
requirement is gone, and here is the number that tells you when to reach for one anyway —
a loop building 100,000 Strings, peak memory:

| | |
|---|---|
| no region | **5,280 KB** |
| `region each { ... }` around the loop body | **1,408 KB** |

Nothing is released until the program exits, so memory grows **in a straight line**. For three
Strings that costs nothing. For a loop over a million rows it costs everything — the arena is a
1 GB reservation, and running out is a named error rather than a crash, but a server loop will
get there.

So the rule of thumb is one sentence: **a region per unit of work whose results you do not
keep.**

```burxt
while more_rows() {
    region row { handle(next_row()); }
}
```

That loop uses the same memory on its first row and its millionth.

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

One rule survives, and it is the one that matters: **a value built inside a `region` block
cannot leave it**, because that block releases at its closing brace.

```burxt
function bad() -> String {
    region r { return "a" + "b"; }
}
```

```
error: cannot return this String: it was built inside a `region` block, which releases at
       its closing brace, so its storage would not outlive the call. Move the allocation
       out of the `region` block, or return a scalar summary.
```

It applies through a **name** as well as directly, and that distinction cost a use-after-free
to learn. This was accepted until v0.0.142 and printed an *empty string*:

```burxt
region inner {
    let s: String = "secret-" + to_string(tag);
    return s;                    // the region releases before the return
}
```

The check asked whether the returned *expression* allocated — and a name is not an
expression that allocates, it is a name that happens to hold one.

It also looks **inside aggregates**: a record holding a built String is itself built, and an
array of them likewise. That arm was missing for four versions and let a use-after-free
through, which is why it walks fields and elements now.

Both refusals, with the compiler's exact words, are spelled out in
[`examples/regions.bx`](../../examples/regions.bx).

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

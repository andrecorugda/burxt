# Burxt — Functions That Allocate In The Caller's Region (M1 amendment)

> Status: **specified, to implement.** A small amendment to
> `M1-MEMORY-MODEL.md`, forced by the thing a Burxt-hosted compiler needs most and
> cannot currently do.

## 0. The wall

A compiler builds messages. Every one of them looks like this:

```text
fn describe(line: Int) -> String {
    return "line " + to_string(line) + ": unexpected byte";
}
```

Today that program cannot be written, and **both** ways out are closed:

```text
error: to_string(...) on an Int allocates, so it needs a region: there is none
       open here.
```

...because a function body has no region open — and if the function opens one:

```text
error: cannot return this String: it was built in a region, so its storage would
       not outlive it.
```

...because the region ends when the function does. So a helper cannot build and
return a String at all. Everything downstream of that is blocked: rendered type
names, error messages, `Int`-to-text in a library function, any string builder that
is not written inline at the point of use.

This is not a gap in regions. It is a gap in what a *function* can say about
regions.

## 1. The observation that makes it easy

**A function called from inside a region already allocates in that region.** The
allocator is a bump pointer; the mark is taken when the region opens and reset when
it closes. A callee that allocates while the caller's region is open puts its bytes
in the caller's region, and they live exactly as long as that region does.

So the value never outlives its region — M1's rule is already satisfied. The
compiler simply had no way to *know* that a given function intends this, and
therefore refused conservatively.

## 2. Decision: declare it, do not infer it

```text
fn describe(line: Int) -> String allocates {
    return "line " + to_string(line) + ": unexpected byte";
}

region r {
    print(describe(3));       // fine: `r` is the region it allocated in
}

print(describe(3));           // compile error: needs a region
```

`allocates` on the signature means: **this function allocates in its caller's
region.** It may build values; it may return them; and every call site must have a
region open.

**Why declared rather than inferred.** Inference is possible — walk the call graph,
mark whatever allocates, propagate — but it would be the only invisible contract in
the language. Every other guarantee Burxt makes is written down at the point it
applies: a rounding contract in the type, `dyn` at the dispatch site, `tail` at the
call, `as scaled` at the boundary. A function that quietly acquires a requirement on
all its callers because someone added a `+` deep inside it is exactly the kind of
action-at-a-distance the rest of the language refuses. Declared, it is also
decidable in one pass, because signatures are already known before any body is
checked.

**Why not a lifetime.** M1's must-NOT list says no lifetimes in signatures, and
this is not one: there is no name, no scope relation, nothing to unify. It is one
bit — *does this function allocate, or not* — which is why it can be a keyword
rather than a parameter.

## 3. The rules

1. `fn f(...) -> T allocates { ... }` may allocate anywhere in its body without
   opening a region. The bytes belong to the caller's region.
2. **Every call to an `allocates` function requires an open region at the call
   site.** Inside another `allocates` function counts: the caller's region is still
   the region in effect.
3. An `allocates` function **may return allocated values.** That is the point.
4. A value allocated inside a `region` block that the function *itself* opened may
   still not be returned — that region does end at the closing brace. This rule is
   unchanged, and now it is the only case the old error describes.
5. A call to an `allocates` function **counts as allocating** at the call site, so
   the caller's own escape rules govern the result exactly as if it had been built
   there. A caller inside a region cannot smuggle the value out of it.
6. A function without `allocates` behaves exactly as before, and its error message
   now names the alternative.

## 4. What this must NOT do

- **NO inference of `allocates`.** If a function needs it, it says so. See §2.
- **NO region names or lifetimes in signatures.** One bit, no parameters.
- **NO implicit region at a call site.** A call that needs a region and has none is
  an error, never an ambient region conjured to satisfy it — that would be M1's
  "hidden global region", which is a GC by another name.
- **NO `allocates` on `extern fn`.** The C side does not know about regions.
- **NO change to codegen.** If this needs new lowering, the reasoning above is
  wrong. It requires none: the bump allocator and the caller's mark already do it.

## 5. Deferred

| Feature | Why deferred | Earns its place when |
|---|---|---|
| `allocates` on methods | `fn` first; methods need the same treatment on the receiver clause | A required program needs an allocating method |
| Inference as an opt-in check | Would be a nice lint ("this could be `allocates`") | The annotation proves burdensome in real code |
| Returning region data *out* of a region block | Genuinely needs region relations | Cross-region references are designed |

## 6. Acceptance

1. `fn describe(line: Int) -> String allocates` compiles, and returns a String
   built from a literal, a concatenation and `to_string`.
2. Calling it inside a region prints the built string; the bytes are the caller's
   region's, and are released when that region closes.
3. Calling it with no region open is a compile error naming the function and
   telling the reader to wrap the call in a region.
4. An `allocates` function may call another one.
5. A non-`allocates` function that allocates still fails, with an error that now
   mentions `allocates` as the fix.
6. Returning a value built inside a `region` block the function opened is still
   refused, with the original message.
7. A caller inside a region cannot return the result of an `allocates` call out of
   that region.

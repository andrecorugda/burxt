# A Burxt program in a browser engine

    ./build.sh

`spec/ROADMAP-2.0.md` §248 files **wasm host glue** beside the Android NDK and iOS signing —
post-1.0, unbuilt, a subsystem. It is `host.mjs`, and this directory is the measurement that
replaced the estimate.

Everything here already exists on a machine that can build Burxt. No emscripten, no wasi-sdk,
no sysroot, no `apt install`.

## What it does

`island.bx` is an ordinary `pure function` returning a `String`. Built natively it prints
HTML; built with `--target wasm32-unknown-unknown` and linked, JavaScript calls it and gets
the same bytes:

```
<article class="receipt"><h1>Receipt for Ada Lovelace</h1><ul><li>Item: One &lt;script&gt; compiler</li>…
```

Note the `&lt;`. The argument was `One <script> compiler`, and nothing in the host escaped it —
`html_text` did, before the bytes left linear memory. **The escaping guarantee survives the
crossing**, and the host cannot opt out because there is no raw path to opt into. That is the
property that makes this worth having rather than a curiosity.

An island is not a new kind of Burxt program. It is a Burxt program with a different `--target`.
What BMX's generator emits from a `.bmx` document is already this shape — see
[bmx.burxt-lang.org](https://bmx.burxt-lang.org/), where the implementation lives now.

## The host is seven to eleven symbols, and two of them do real work

There is no single number, and the differences are worth knowing before you write a shim.
Measured with `llvm-nm -u`:

| island | symbols | needs a varargs walker? |
|---|---|---|
| all-`String` parameters, exports only the view | **7** | no |
| renders an `Int` or a `Decimal` | **8** — adds `snprintf` | **yes** |
| keeps a `region main` calling `print` (both files here) | 10 | yes, plus `printf`/`putchar` |

In every case the same breakdown holds:

| symbol | what the host owes it |
|---|---|
| `malloc` | a bump allocator. **Return 0 on failure**, see below |
| `memcpy` | one line |
| `snprintf`, `printf`, `putchar` | a varargs walker. **Zero-padding is not optional** — see below |
| `getrlimit` | one truth: the link-time stack size |
| `exit`, `fprintf`, `fwrite`, `fputs` | end the program. Panic path only — stubs returning 0 are enough, and they never fire |
| `stderr` | a **data** symbol, not a function |

**Two functions do real work: `malloc` and `memcpy`.** That is true of every shape.

The middle row is the one that matters, and it is easy to talk yourself out of. An island whose
parameters are all `String` needs no integer formatting — but if JavaScript is formatting the
price, the guarantee was given away before the call. `money-island.bx` keeps the money as money:
a `Decimal<2>` crosses as its exact scaled integer and becomes text inside the module.

`wasm32-unknown-unknown`, so **no WASI**. The target has emitted objects since v0.0.197; only
the shim was missing.

## The trap that does not announce itself

`to_string` on a `Decimal<2>` renders through `"%s%llu.%02llu"`. A varargs walker that handles
`%llu` but ignores the `02` turns **$1299.05 into `1299.5`** — no crash, no warning, money wrong
by a factor of ten. This file's shim did exactly that in its first version, and nothing caught
it except rendering the same value natively and in wasm and comparing the two.

That is why `build.sh` diffs both islands against their native output rather than just printing
them. **A shim that is nearly right about `printf` is a shim that silently corrupts prices.**

## Four things that cost an hour each to find

**`rust-lld` is `wasm-ld`.** `lld` is usually not installed, but the Rust toolchain ships one:
`~/.rustup/toolchains/*/lib/rustlib/*/bin/rust-lld -flavor wasm`. This is the single fact that
makes the rest an afternoon rather than a toolchain project.

**`--allow-undefined`, not `--import-undefined`.** The latter turns undefined *functions* into
imports and leaves data symbols alone, so the link fails on `stderr` — three identical errors
naming a symbol that looks like it should already be handled.

**A 4 GiB reservation cannot work here, and the reason generalises.** `burxt.alloc` used to ask
`malloc` for a flat 4 GiB, on the argument that the reservation is virtual and lazily committed
so raising it costs nothing resident. That is true of a 64-bit OS and false of wasm twice over:
4 GiB is the *entire* wasm32 address space, and `memory.grow` **commits**, so a reservation
here is resident rather than virtual. The allocator now asks for 4 GiB, then 256 MB, then
16 MB, and keeps whichever it gets — which is why `malloc` in `host.mjs` must return 0 rather
than throw. The same change fixed a defect that was never about wasm: the old code never
checked `malloc`'s answer at all, so a failed reservation stored a null and let the next write
find out.

**The stack-overflow guard wrapped.** `getrlimit(RLIMIT_STACK)` fills a floor as
`base - (size - 128 KB)`. A linear-memory stack sits near address zero, so that subtraction
underflowed to a colossal unsigned number, putting the floor above every real stack pointer —
and every call reported "this call went too deep" before the program ran a line. The
subtraction saturates now. The same wrap happens on *any* platform if `getrlimit` fails, since
its return value was ignored and a zeroed `rlim_cur` passed the sanity check.

## What this does not do yet

- **No DOM.** The host reads a `String` and it is the page's business what to do with it.
  Hydration, event wiring and the reactivity question all sit above this line, and none of
  them are decided.
- **No `fetch`, no timers, no storage.** Anything a Burxt program `touches` needs a host
  function, and only the eleven above exist. This is a feature at the island boundary: a
  `pure` view reaches nothing, and `burxt effects --allow ""` says so before it ships.
- **16 MB of region memory** on the fallback rung, once, for the life of the module. Ample for
  a view; a program that wants more must ask for it, and there is no chunk chaining.
- **Not measured in a browser.** node is a WebAssembly engine and this is engine-level code,
  but that is a reasoned claim and this file is here because reasoned claims about platforms
  were wrong four times this month. Treat it as unproven until someone loads it in Firefox.

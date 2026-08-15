---
title: What Burxt does not do
---

> **This documents Burxt 1.0.0, and it is frozen.** It will not change again — that is what makes
> it useful to someone still on 1.0. The current documentation is at
> [the top of the site]({{ site.baseurl }}/).


# What Burxt does not do

Every language has gaps. Most projects let you find them one at a time, in your own code, at the
worst moment. This page is the other approach: **everything deliberately absent, and everything not
built yet, in one place, with the reason.**

Two kinds of entry, and the difference matters more than the list:

- **Decisions** — these are not coming. Each was chosen, each has a reason, and changing one would
  change what the language is.
- **Not yet** — real gaps with real plans. If one blocks you, it is a legitimate thing to ask about.

If something is missing from *this page* rather than from the language, that is a bug and worth
reporting. A gap document that is wrong about a gap is worse than none, because it will be believed.

---

## Decisions — not coming

### No floating point

There is no `Float`, and there will not be one. Money is `Decimal<scale>`, an exact scaled integer.

This is the one that surprises people, so here is the strongest evidence for it: the flagship case
nobody thought was reachable without floats — **vector similarity search** — turned out reachable
*and better*. Exact `Decimal<7>` cosine similarity carries a claim no float-based store can make:
**scores are byte-identical on every machine, every target and every run.** That is not a workaround
for the missing feature. It is the feature.

If you need transcendental maths on physical quantities, this is the wrong language and that is fine.

### No null, no truthiness, no `unwrap`

Absence is `Option<T>`, failure is `Result<T, E>`, and conditions must be `Bool`. There is no
`unwrap` because a name that means *"crash here, later, in production"* should be as inconvenient as
what it does.

### No GC, no reference counting, no runtime

Memory is regions: every block is one, and a block releases what it allocated when nothing outside can
still reach it. **No pauses, no finalizers, no background thread.** Release is one pointer assignment.

The cost is stated in [the memory guide](guide/04-memory.md): region granularity is coarser than
Rust's borrow checker. A value the analysis cannot prove safe to release simply is not released — the
failure direction is memory, never a dangling pointer.

### No inheritance

Composition and interfaces only. Dropped deliberately; `dynamic Trait` covers what polymorphism is
actually used for.

### No closures

This one is recent and worth explaining, because "no closures" usually means "not yet."

`dynamic Trait` **is** a function value — passable as a parameter, callable, and it captures state in
the implementor's fields. `interface Predicate<T>` plus a class that implements it does what a
closure does, with the captured state written out where a reviewer can see it. `map`, `filter`,
`fold` and `sort_by` are all written this way in `lib/fn.bx`.

Closures were *buildable* when this was decided — the memory question that had blocked them was
answered — and declined anyway, because a second way to express one thing costs every reader the
question of which one they are looking at.

### No `unsafe`, no reflection, no conditional compilation

No escape hatch, no runtime type inspection, no `#ifdef`. A program means one thing on every platform.

### No catch or recover

A failed contract, an overflow, an out-of-range index: the program prints a named error and exits 70.
There is no handler and no unwinding. **Every failure is named** — this is the one guarantee the
language treats as non-negotiable, and it is why `sdiv` is not allowed to fault silently.

### Smaller shape decisions

Bitwise operations are seven named builtins, not operators · String ordering is **byte** order, never
locale collation · no operator overloading · no C-style ternary · `%` is the percent literal, so
modulo is `remainder(a, b)` · no format-spec mini-language inside interpolation · no implicit prelude
or glob imports · `Map` iteration order is defined and never "unspecified" · no wildcard `_` match arm,
because a new enum variant should break the code that ignored it · contracts are **never** stripped,
in any build mode.

### Rust is not going away

Burxt compiles Burxt to a byte-identical fixpoint, so self-hosting is proven rather than aspirational.
The Rust compiler stays anyway, as the **trust anchor and the differential**: two independent
implementations that must agree. That disagreement is the single most productive bug-finding tool this
project has — it has caught defects no test suite saw, including memory corruption that every green
suite missed.

---

## Not yet — real gaps

### No dependency management

There is no manifest, no lockfile, no registry. `use "path.bx"` resolves textually, relative to the
file. You can vendor code; you cannot depend on a version of it.

There is also no visibility marker yet — every declaration in a file a program `use`s is visible to it.
That changes with dependency management: the keyword is **`public`**, spelled out like everything else in
this language, and the boundary is the **package** rather than the file. `use` concatenates sources into one
buffer, so a file boundary does not exist at runtime for anything to be private across; a package boundary
will. Everything stays visible inside a package, and only `public` declarations are importable by a package
that depends on it.

### No concurrency

No threads, no async, no channels. A Burxt program is single-threaded.

This is deliberate sequencing rather than disinterest. The intended claim is that *two threads cannot
corrupt a balance*, derived from a declared invariant rather than from a lock the programmer
remembered — and shipping that half-done would be worse than not shipping it.

### No sockets, no TLS, no HTTP

A program can read files, run commands and read standard input. It cannot open a network connection.
Anything HTTP-shaped goes through `os_run` and a subprocess today.

### C interoperation is limited on purpose

You can call C functions that take and return scalars, strings and opaque pointers. **C cannot call
back into Burxt**, which is what stops `sqlite3_exec` and most iteration APIs. The prepare/step/column
path works; the callback path does not.

### Cryptography: some built, most deliberately not

Hashing, HMAC, PBKDF2, hex and base64 are **built** — `lib/hash.bx`, `lib/encoding.bx` and
`lib/secure.bx` — because they have published test vectors and no secret-dependent branching, so
"it compiles" and "it is correct" are the same statement, checkable against the values the standards
publish. SHA-256, SHA-512, HMAC, PBKDF2, CRC-32, FNV-1a, hex, base64 and base64url, every RFC 4648
vector pinned in a fixture. Entropy comes from `getentropy`, with `secure_uuid_v4` and
`secure_token_hex` over it, and `string_equals_constant_time` for comparing any of it.

*(This paragraph said "being built" for two versions after they landed. A gap document that is wrong
about a gap is worse than none — including when it is wrong in the direction of modesty, because the
reader who needed a digest went and shelled out for one.)*

**AES, ChaCha20, RSA, Ed25519, X25519, TLS, Argon2 and bcrypt are deliberately absent and will be
bound rather than written.** Two reasons: Burxt gives no control over instruction timing or cache
behaviour, and RSA and the elliptic curves need arbitrary-precision integers, which the language does
not have — `Decimal` is a scaled i64 capped at scale 18. Hand-rolling them would produce ciphertext
that looks perfectly fine, which is exactly the silent wrong answer this language exists to refuse.

### A secret cannot be zeroed

There is no `zeroise`. A String lives until its region closes, so a key stays resident after the code
using it has returned. Fixing this properly needs a primitive that writes over region memory. Saying
so is the minimum.

### Unicode is ASCII-correct, and the names say so

`string_to_upper_ascii` is spelled that way because it is what it does: non-ASCII passes through
unchanged. Full Unicode case mapping needs ~1,400 generated entries plus locale edges — Turkish `ı`,
and `ß` becoming `SS`, one codepoint becoming two.

Codepoint iteration, `char_count`, `from_codepoint` and `is_valid_utf8` are all present. **A String is
UTF-8 and that is checked at every entry point** — `read_file`, `argument` and `c_string_at`, which is
also how `os_env` is covered — so invalid bytes are a named error naming the door they came through,
rather than a corrupted value discovered later somewhere that can only report the symptom. Overlong
forms, surrogates, anything above U+10FFFF and a sequence the buffer cut short are all refused.

This sentence was here before the check was, which is worth admitting rather than quietly correcting:
the guarantee was published and unenforced until v0.0.284, and a published guarantee is exactly the
kind a reader stops verifying.

**The cost, stated plainly: you cannot read binary through `read_file` any more.** `file_read_bytes`
answers `Option<[Int]>` and is the door for data that is not text.

### Nothing runs on a phone or in a browser yet

Objects compile for eight targets including `wasm32`, `aarch64-apple-darwin` and
`x86_64-pc-windows-msvc`, and **the IR is byte-identical across all of them**. What is missing is
linking: a sysroot per platform, Android's NDK and JNI shell, iOS signing, and a wasm host. That is
packaging work rather than compiler work.

### Other absences

No formatter (`burxt fmt`) · no regex · no stack trace on failure, only the named error and its
location — though as of v0.0.285 there is no longer any failure WITHOUT one: a runaway recursion
names itself and exits 70 like everything else, where it used to die of a bare SIGSEGV · no default parameter values or named arguments · no `if` as an expression · no attributes ·
**no warnings about your program** — every diagnostic the compiler can produce about Burxt code is a
refusal to compile, so there is no way to flag something without stopping the build · parse errors
arrive one at a time, with no token-stream recovery.

The one thing that prints the word *warning* is the driver, not the compiler: `-g` without `-O0`
says so, because a line table over optimised code is honest about instructions and misleading about
statements. That is a remark about how you invoked the tool, not a judgement about your program, and
the distinction is the whole of why the rule above still holds.

---

## How to read this page

The list is long because it is honest, not because the language is thin. Every entry above is either
a decision with a reason or a gap with a plan, and the ones we consider most limiting for real work
today are **dependency management and the network** — in that order.

The debugger used to head that list, and it is gone: `burxt build -O0 -g` emits DWARF, so a debugger
stops on the line you wrote, names the function, prints your locals as values rather than addresses,
and can break on a `requires` clause and show you the arguments that violated it.

What you get in exchange is narrow and unusual: **exact decimal arithmetic with no floating point
anywhere, contracts that are always checked, no runtime, no garbage collector, memory that is released
when a block ends, and two independent compilers that must agree on every program.**

If that trade is wrong for what you are building, this page has done its job.

---
title: What Burxt does not do
---

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

### No package registry, and no transitive resolution

**Most of this section used to say "no dependency management" and was wrong on three counts.** There
is a manifest — `burxt.package`. There is a lockfile — `burxt fetch` writes `burxt.lock`, pinning
exact commits. And there is a visibility marker: `public` is a keyword in both compilers, the
boundary is the **package** rather than the file, and reaching a declaration that is not `public`
is a named refusal. Two packages depend on Burxt this way today.

What is actually missing:

- **No registry.** A dependency is a git URL and a commit, written out. That is an operational
  commitment rather than a language feature, and it is not scheduled.
- **Resolution is FLAT.** Only the root manifest's dependencies are read, so a package cannot bring
  its own: an application depending on star-burxt must also declare `bmx` itself, under the same
  name star uses. One level from breaking quietly, and the fix is scheduled rather than done.
- **No version constraint.** A manifest pins a commit; it cannot say `requires burxt >= 1.3`. The
  first thing that needs it is a package's own CI, not a user.

### No threads. Processes, yes

No threads, no async, no channels. `os_fork` and `os_wait_for_child` in `lib/os.bx` are what a Burxt
program has for doing more than one thing at once, and a pre-forked server is written with them
today.

Threads need `pthread_create`, which takes a **function pointer** — C calling back into Burxt, which
is the door below that is genuinely still shut.

The sequencing is deliberate rather than disinterested. The intended claim is that *two threads
cannot corrupt a balance*, derived from a declared invariant rather than from a lock somebody
remembered, and shipping that half-done would be worse than not shipping it. Separate processes get
the same guarantee the coarse way: two workers share no memory, so there is nothing to corrupt.

### TCP and HTTP, yes. TLS by binding. No DNS

`lib/net.bx` opens, binds, accepts, reads and writes TCP sockets, both as a server and as a client.
A Burxt program answers HTTP requests. What is missing above it:

- **No TLS *library*** — and writing one is not the plan. Binding one is the recorded decision: this
  language gives no control over instruction timing, and a hand-rolled handshake that *looks* fine is
  exactly the failure it exists to refuse.

  **Binding one already works, with no compiler change.** Measured 2026-08-18: six
  `external function` declarations against OpenSSL, built with `burxt build client.bx -lssl -lcrypto`,
  completing a **TLS 1.3** handshake to a public host and reporting the negotiated version. So the gap
  is `lib/tls.bx`, a wrapper nobody has written, rather than a capability the language lacks — and
  this bullet said otherwise until it was measured, one paragraph above the note recording that the
  same thing had already happened to the sentence about opening a connection at all.
- ~~**No HTTP**~~ — **`lib/http.bx` ships it**, both halves, over the sockets that already existed.
  A request is parsed into an `HttpRequest` (method, path, decoded query, headers, body), a
  `Handler` interface is what a server takes since Burxt has no function values, and a client
  answers an `HttpResponse`. What is still absent inside it, named rather than discovered: **no
  chunked transfer encoding** (a body is read by `Content-Length` or REFUSED, never truncated),
  **no keep-alive** (one request per connection, and `Connection: close` is sent so a client knows),
  and no DNS, so a client takes four octets.
- **No DNS**, and this one is a single missing builtin rather than a design problem. An address is
  four octets today — `net_connect_ipv4(93, 184, 216, 34, 80)`.

  Measured 2026-08-18: **`getaddrinfo` itself already succeeds from a Burxt program**, returning `0`
  for a real hostname, and `c_bytes_at` reads back the eight bytes it wrote — a genuine heap address.
  What cannot be done is turn those eight bytes into a `CPointer` again, so the `addrinfo` chain
  cannot be walked. `c_string_at`, `c_bytes_at` and `c_bytes_to` all cross the wall; the mirror that
  reads a POINTER field out of a C struct does not exist yet.
- **Blocking, with no timeouts.** `net_accept` waits.

This section said *"It cannot open a network connection"* until it was measured. It could: every
call but `bind` worked with no compiler change, and `bind` needed one builtin.

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

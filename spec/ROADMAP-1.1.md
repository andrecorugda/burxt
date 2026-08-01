# Burxt 1.1 — the release after the core

**Status: the plan of record for 1.1.** Created v0.0.260 as a hosts-only document; Part II
added 2026-08-01, when 1.1 acquired a second half.

[`ROADMAP-1.0.md`](ROADMAP-1.0.md) is the road to *a language someone outside this repository
can ship on*. This file holds what comes after it, and it is **two unrelated things**:

| | What it is | Why it is not in 1.0 |
|---|---|---|
| **[Part I — Hosts](#part-i--the-hosts-and-what-each-one-costs-to-verify)** | Making the compiler itself run somewhere new | Cannot be **finished** by writing it — it needs hardware nobody here has |
| **[Part II — The web stack](#part-ii--the-web-stack)** | HTML, CGI, sockets, a real server | Cannot be **started** until the core is done. Andre's call, 2026-08-01 |

They share no machinery and neither blocks the other. They are in one file for one reason: a
reader asking *"what is in 1.1"* has to find all of it in one place, and a `ROADMAP-1.1` that
listed half the release would be the exact kind of index drift this project keeps tripping over.

---

# Part I — the hosts, and what each one costs to verify

Part I holds the distribution work that **cannot be finished by writing it**, because finishing
means proving it on hardware or in an environment nobody here can reach.

That split is the whole point. Everything quick enough to build and verify in one pass went
into 1.0's §H and is done. What is left is the work whose honest state is *"plausible, and
unproven"* — and the failure mode this part exists to prevent is a roadmap row that says DONE
because the code was written.

## The distinction that governs Part I

Two things get called "supporting Android", and only one of them is hard.

| | What it means | State |
|---|---|---|
| **Target** | You compile a Burxt program **for** the platform, from a machine that already works | **DONE.** Verified in `the_ir_is_the_same_for_every_target` |
| **Host** | The `burxt` compiler **runs on** the platform | The subject of Part I |

Burxt emits correct objects for **thirteen** triples today, each measured rather than assumed
— including all three Android ABIs, `aarch64-apple-ios` and `wasm32-wasi`. The IR is
byte-identical for every one, which is what makes the decimal answers identical too.

**Do not let a "we support Android" row hide which of the two it means.** Every row below says
host or target in its first sentence.

---

## H — where 1.0 left it

For reference, because 1.1 only makes sense against what already shipped:

- Four **hosts** built natively and published per tag: `linux-x86_64`, `linux-arm64`,
  `darwin-arm64`, `darwin-x86_64`.
- A multi-arch **OCI image** (`amd64` + `arm64`), smoke-tested by actually running it before
  the manifest is pushed.
- **Windows is served by that image**, via WSL container — see W1.

---

## W — Windows

### W1 — Windows is DONE, by container, and that is a decision not a shortcut

**Host.** Windows 11 ships `wslc.exe`, a built-in OCI runtime: Linux containers natively, no
Docker Desktop, no third-party runtime, Docker-compatible CLI, GPU support from day one.
Announced at Build 2026, public preview 29 June 2026, GA targeted fall 2026.

So `wslc run ghcr.io/andrecorugda/burxt run hello.bx` is the Windows story, and it costs one
line of documentation against an image that had to exist for Kubernetes anyway.

**The trigger that would reopen this:** a Windows user who needs `burxt.exe` on `PATH` outside
a container — a CI runner that cannot nest containers, an IDE integration that shells out, or
an installer that must not require WSL. Until one of those is real, W2 is not worth its price.

### W2 — A native MSVC port — NOT scheduled, and here is the bill

**Host.** Three separate ports, not one:

1. **inkwell + LLVM 18 under MSVC.** The Linux and macOS builds get LLVM from apt and Homebrew.
   Windows has neither; `llvm-sys` must find an MSVC-built LLVM 18 with the static libraries,
   and that is the step that historically eats the week.
2. **The link step.** `burxt build` shells out to `cc`. On MSVC there is no `cc` — it is
   `link.exe` with entirely different flag spelling, or `clang-cl`. This is compiler surface,
   not packaging: `main.rs` currently hands everything after the source file to the linker
   *unchanged*, and that contract does not survive the translation.
3. **Install and library paths.** `scripts/install.sh` is POSIX shell and assumes
   `/usr/local`. Windows needs its own path, and then there are permanently two.

**Cost of the third one is the real argument.** The first two are finite. A second install
surface is forever, and it goes stale the way six spec headers went stale.

**Verification it would need:** the full suite green on `windows-latest`, the tarball smoke
test rewritten in something that is not `sh`, and `the_release_tarball_works_without_rust_or_llvm`
given a Windows equivalent. None of that can be checked from here.

---

## N — Android as a host

### N1 — Running the compiler on the phone — an EXPERIMENT, not a wall

**Host.** Termux, `burxt build` typed on the device.

The version objection is gone: **NDK r27 bundles LLVM 18**, exactly what inkwell's `llvm18-1`
feature wants. But that toolchain *runs on x86-64 and targets Android* — `llvm-sys` needs LLVM's
static libraries built to **run on** aarch64 bionic, and the NDK does not ship those. Termux has
its own `llvm` package, which may be the missing piece.

**This is written as an experiment on purpose.** Six times in this project a wall that looked
like a design constraint dissolved without new machinery, and the rule that came out of it is
*measure the error, do not reason about it*. Nobody has run the build and read the linker error.
Until someone does, "impossible" is an opinion.

**The measurement, in order:**

```sh
# on the device, under Termux
pkg install llvm rust binutils
LLVM_SYS_181_PREFIX=$(llvm-config --prefix) cargo build --release
```

**Report the exact failure, not a summary.** A missing `libLLVM*.a` is a different problem from
a bionic symbol mismatch, and only the second is genuinely fatal.

**What it is worth if it works:** less than it looks. CHN hosts on Termux because it is 830 KB
of C with no dependency; Burxt would be an 18 MB binary compiling on a phone CPU. The people
served are people writing Burxt on a phone. Shipping Burxt programs **to** Android is the
commercially real one and it already works.

**Trigger to promote this above W2:** anyone asks for it. Nobody has.

---

## D — macOS, and the bug the matrix found on its first run

### D1 — `stderr` does not exist on Darwin — MEASURED, one symbol, blocks both macOS hosts

**Host and target, both.** The four-host matrix was dispatched before any tag, and
`darwin-arm64` failed the suite immediately. Every failing test was a test that *links*:

```
Undefined symbols for architecture arm64:
  "_stderr", referenced from:
      _bx.at_byte in burxt-12623-main.o
ld: symbol(s) not found for architecture arm64
```

`codegen.rs:3441` emits `add_global(ptr, None, "stderr")`. On glibc `stderr` is a real
exported symbol. On Darwin it is **not**: `<stdio.h>` defines `stderr` as a macro for
`__stderrp`, and nothing named `stderr` is exported at all. One hardcoded symbol name,
and it takes down every linking test on macOS at once.

The shape of the fix — **not applied here, see the ownership note below**:

```rust
// Darwin's libc exports no `stderr`: <stdio.h> makes it a macro for `__stderrp`.
let name = if triple.contains("apple") { "__stderrp" } else { "stderr" };
let stderr_g = self.module.add_global(ptr, None, name);
```

### D2 — why no test caught it, and the design question that follows

**This is the part worth more than the fix.**

`the_ir_is_the_same_for_every_target` covers thirteen triples including
`aarch64-apple-darwin`, and it passes. It passes **because** the bug is uniform: the test
compares IR across targets after dropping the `target triple` and `target datalayout` lines,
so a global named `@stderr` appears identically everywhere and reads as agreement.

*A test for sameness cannot see an error that is the same everywhere.* The IR-equality test
was the wrong instrument, and it will stay the wrong instrument for this class of bug.

So `--target x86_64-apple-darwin` has been emitting objects that **cannot link on a Mac**
since cross-targeting shipped in v0.0.197, and the tarball README now names Darwin among the
verified triples. That claim is currently **false for linking** and true only for emission.

**The design question this forces**, and it is a real one rather than a bug report: the
project's stated property is that *the IR is byte-identical for every target, which is what
makes the decimal answers identical too*. But libc symbol names are platform-dependent by
nature. Those two cannot both be absolute. The resolution is probably that the guarantee
covers **the arithmetic** — every decimal operation, every rounding helper, every overflow
check — and explicitly excludes libc interface symbols. That exception should be **written
into the guarantee**, because an unqualified claim that is quietly untrue is worse than a
narrower one that holds.

**What must be true before macOS is called supported:**

1. The symbol chosen by target, not hardcoded.
2. A **link** test, not an IR-comparison test — the existing instrument is structurally blind
   here. Emitting an object proves nothing about whether it links.
3. The full suite green on `macos-14` and `macos-13` runners.
4. The guarantee in §D2 restated with its libc exception.

**Ownership note.** `src/rust-compiler/codegen.rs` was staged by another session while this
was diagnosed, so the fix was deliberately **not** applied — see `.claude/SESSION-CLAIMS.md`.
Diagnosing and handing over beats two sessions editing one file.

### D3 — the matrix stays at four hosts

The two macOS entries are **left in and left failing**. Removing them would make the release
green by making it silent, and `publish` deliberately refuses to attach anything unless four
tarballs arrive — precisely so a release cannot ship one platform and look complete.

**A red matrix that names the missing platform is worth more than a green one that hides it.**

---

## L — What the container found out about the language

### L1 — `use` has no search path, and the image is where that first hurts

**Compiler, not packaging.** `use` resolves **relative to the importing file's directory and
nothing else** — no search path, no environment variable (`load_into`,
`src/rust-compiler/main.rs`). So inside the image the standard library must be named in full:

```burxt
use "/usr/local/lib/burxt/string.bx";
```

A `BURXT_LIB` environment variable was drafted into `scripts/Dockerfile` and **removed before
it shipped**, because the compiler never reads one and an image advertising a variable that
does nothing is worse than one stating the real path.

**Why the container is what raised it.** On a laptop the library's location is a user's choice
and a search path would need a policy. In the image it has exactly **one** fixed home, which is
the first context where a search path has an unambiguous answer.

**Why it is not scheduled here.** It is a language change and it collides with a decision on
record — *no implicit prelude, no glob imports*. A search path is one step from an implicit
prelude, and the reason to want it (typing) is the weakest kind of reason this project accepts.
The correctness argument, if there is one, is that an absolute path in source is not portable
between the tarball and the image — and **that** is worth a spec.

**Must NOT do:** add a search path that makes `use "string.bx"` resolve differently depending on
where the compiler was installed. Two machines compiling the same file must compile the same
program. Any design that cannot promise that is refused.

---

## G3 — what Part I does NOT cover

[`ROADMAP-1.0.md`](ROADMAP-1.0.md) §G3 is *M3 packaging — per-target linking, desktop matrix,
Android NDK/JNI, iOS signing, wasm host glue*, and it stays post-1.0.

**G3 and Part I are about opposite directions**, which is why the work did not simply merge:

- **G3 is target-side.** Given an emitted object, produce a runnable artifact *for* that
  platform — a sysroot, a linker, an iOS signature, wasm host glue. Objects already emit for
  thirteen triples; what remains is everything after the object.
- **This file is host-side.** Make the compiler itself run somewhere new.

The four hosts and the image shipped without touching G3 at all, because none of them needed a
sysroot. Anyone reading G3 as "packaging is post-1.0, so the macOS build must wait" has the
split backwards.

**And Part II is neither.** The web stack is not target-side and not host-side — it is library and
language work that runs on a host already supported, for a target already emitting. It appears in
this file because it ships in 1.1, not because it belongs to the target/host axis at all.

---

## Verification — the rule Part I is built around

Every row in Part I states *what would have to be true* and *on what machine*, because the failure
it exists to prevent is a DONE that was never executed anywhere.

- **A cross-target claim needs a runner invariant**, not a sentence. The thirteen triples are in
  `the_ir_is_the_same_for_every_target`; they were added in the same version the tarball's README
  began naming them.
- **A host claim needs the suite green on that host.** The four in 1.0 run `cargo test --release`
  and the H4 tarball gate on their own runner. A host that cannot do both is not supported.
- **An image claim needs the image RUN.** `release.yml` builds `linux/amd64`, executes
  `19.99 * 3`, and refuses to push unless it prints `59.97`. Building a manifest proves nothing;
  an image missing `cc` builds perfectly and fails every `burxt build`.
- **NOT DONE is not evidence.** A stale limitation is worse than a stale DONE, because nobody
  re-tests what the document says does not work. N1 is an experiment with a command in it for
  exactly this reason.

---

# Part II — the web stack

**Detail lives in [`M15-WEB.md`](M15-WEB.md).** This section is the summary a reader of the 1.1
roadmap needs; the design, the measurements and the refusals are there.

**Nothing here is built.** Andre's call, 2026-08-01: 1.0 is the real core and comes first.

## Why it is in 1.1 at all

Burxt has no web story, and *"how does Rust handle front-end?"* is a question with no answer in
this repository today. Rust's answer is instructive: it put **nothing** in the language and got
Axum, Actix, Askama and Maud from people who were not on the compiler team. PHP put the web *in*
the language and spent twenty years unable to remove any of it.

So the goal of Part II is not a Burxt web framework. It is **the primitives someone else builds one
on** — which is how a language nobody has heard of acquires an ecosystem, and an ecosystem is the
point.

**Must NOT do:** ship a router, a template file format, or `burxt new --web`. The day one exists,
every framework author is competing with the compiler team instead of building on it.

## The split that decides the order

"Front-end" is two unrelated problems wearing one word, and only one is hard:

| | Needs | State |
|---|---|---|
| Producing HTML | strings, an escape table, a recursive render | **Needs nothing. Measured working 2026-08-01** |
| Serving it over a socket | C struct layouts, sockets, a concurrency model | The rest of the table |

| Slice | What | Depends on | Language change |
|---|---|---|---|
| **W0** | `lib/html.bx` + `lib/cgi.bx` | **nothing** | none |
| **W1** | C struct layouts — enough to describe `sockaddr_in` | A7 widths (**DONE** v0.0.261) | compiler |
| **W2** | `lib/net.bx` — the socket calls | W1 | none |
| **W3** | Concurrency — this is **`ROADMAP-1.0.md` §G1**, not a new item | M1's re-decision | compiler, large |
| **W4** | `lib/http.bx` — request, response, listener | W2, W3 | none |
| **W5** | TLS / HTTPS | W4, §E build-vs-bind | undecided |

**Anyone reading "the web waits on threads" has it wrong for half the stack.** W0 is numbered zero
because it sits outside the dependency chain: `lib/html.bx` is `lib/json.bx`'s shape applied to a
different grammar, and `lib/cgi.bx` needs only `os_env` and `os_read_all`, which already exist. A
Burxt binary behind nginx serves dynamic pages with no listener and no concurrency — which is how
PHP started, and is a complete deployment story rather than a lesser one.

## Two rows this part fixes, and only one of them was wrong

Exploring the web stack sent someone back to [`FAR-HORIZON-ROADMAP.md`](FAR-HORIZON-ROADMAP.md) §1,
and the honest result is one correction and one sharpening — **not two errors**, which is what the
first draft of this section claimed:

- **C structs — genuinely STALE.** The row said a C struct is out of reach because it *"needs
  widths"*. Widths landed in **v0.0.261**, and A7's own *unblocks* column names C structs. The
  blocker was gone and the row still described it. Corrected.
- **Sockets — accurate, but vague.** The row said *"nothing wrapped. A fd is an int so it is
  reachable, but no library"*, verdict Blocking. That was **right on both counts** and stays
  Blocking, because you still cannot write a network program. What it did not say is *where* the
  boundary falls: `socket`/`send`/`recv`/`listen`/`close` cross today, and only `bind`/`connect`/
  `accept` need the struct. Sharpened, not corrected.

The first is Part I's own rule applied to a different file — *NOT DONE is not evidence*, and nobody
re-tested the row once A7 shipped. The second is a reminder that "this document is out of date" is
itself a claim worth checking before publishing.

## Verification

`M15-WEB.md` §4 carries the per-slice table, built on the same rule as Part I — what would have to
be **executed**, not written. The one worth repeating here: **W0 will be tempting to mark DONE the
moment `html.bx` compiles, and compiling proves nothing about escaping.** The fail fixture is the
test that matters.

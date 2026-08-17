# Burxt 1.2 → 1.5 — the road to the playground

**Status: the plan of record for the 1.x line after 1.1.** Opened 2026-08-16, once 1.1.0 had
shipped sockets and processes. Four releases, not one; see the table below for why.

The goal is one thing, and everything here is chosen because it stands between us and it:

> **`play.burxt-lang.org` — a Burxt program, serving a page written in BMX, running strangers'
> code without trusting it.**

`spec/1.0/ROADMAP-1.0.md` §H12 records the playground as Andre's, 2026-08-15. This is that row
becoming a plan.

**This file lives at `spec/` root while it is active**, the way `ROADMAP-1.0.md` did. Each release
moves its own shipped spec into `spec/1.2/`, `spec/1.3/` and so on as it lands, and this file joins
the last of them when the line closes.

---

## Why these are minors, and why each is its own release

Two questions, and the compatibility promise answers the first before anyone argues:

> **A patch adds nothing.** Not a function, not a keyword, not a flag. If it adds anything, it is
> a minor, and calling it a patch would break the only rule a version number carries.

Every row below adds surface — a CLI verb, library functions, whole modules. **None of it can be
a patch.**

Neither is any of it a **major**, and the same document is equally precise: a major is *"a program
that compiled may stop compiling, or may compile and mean something different."* Nothing here
breaks a program. `burxt effects` is a new verb; `os_limit_cpu` is a new function; `lib/html.bx`
and `lib/cgi.bx` are new modules. A program written against 1.1 compiles unchanged against every
one of them.

**One thing here IS a major and it is worth stating rather than discovering.** `lib/bmx.bx`
existed on `develop` and has moved to BMX's own repository — but it was never in a release, so
removing it breaks no published program and costs nothing. That was true only while it stayed
unreleased: `docs/compatibility.md:20` makes removing a shipped `lib/` module a major, so a tag
cut before the move would have made the move itself a breaking change. It is why the release was
held until the migration finished.

So: **four minors, shipped separately** — Andre's call, and the right one. Each is large enough to
be understood on its own, and bundling four unrelated capabilities into one number tells a reader
nothing about which of them they are upgrading for.

| release | what it is | why it is separately shippable |
|---|---|---|
| **1.2.0** | **The view layer** — `html`, `cgi`, `bmx` | Already written and green. Nothing else waits on it |
| **1.3.0** | **`burxt effects`** — §Q1 | The safety gate. Useful to CI on its own, with no playground in sight |
| **1.4.0** | **Bounding a stranger's program** — the `rlimit` family | Useful to anyone running a subprocess, playground or not |
| **1.5.0** | **The playground** — `play.burxt-lang.org` | The application. Needs all three above |

**Documentation is not one of them.** It ships inside whichever release touches it — the rule this
repo already learned the hard way, at `spec/1.0/ROADMAP-1.0.md` §H2: *fix each in whichever
version touches it, never as a separate cleanup, which is how they rotted.* There is no
documentation release in this plan and there should never be one.

**Documentation and specs are versioned by SERIES, never by patch** — `1.0`, `1.1`, `1.2` — the
model Laravel uses. `docs/_config.yml` already carries `burxt_series` for it. A reader picking a
version wants the line they are on, not the fourteenth patch of it.

## What is already true, measured 2026-08-16

Written down because half of what this release needs turned out to exist, and a plan that
re-specifies working code wastes the release.

| | |
|---|---|
| **TCP, server and client** | `lib/net.bx` — 1.1.0, all four platforms |
| **Processes** | `os_fork`, `os_wait_for_child`, `os_flush` — 1.1.0 |
| **CGI** | `lib/cgi.bx` — request from the environment, response to stdout |
| **A typed HTML tree** | `lib/html.bx` — escaped on render, `Html.Raw` a separate variant |
| **`setrlimit` from Burxt** | **measured**: a parent forked a child under `RLIMIT_CPU`, the child span forever, the kernel killed it, the parent survived and reaped it. Works because `c_bytes_to` shipped — an `rlimit` is a struct by pointer, exactly like `bind`'s `sockaddr_in` |
| **Effects propagate and cannot be hidden** | **measured**: three layers deep, `wrapper` calling `sneaky` calling `system` is still refused until every one of them declares `touches commands` |

**So concurrency is not on this list and is not a blocker.** Threads need C calling back into
Burxt and that door is shut. A playground forks per submission anyway — it must, to run untrusted
code — and separate address spaces are what make that safe. Under CGI, concurrency belongs to the
web server in front.

---

## The architecture, decided

**CGI behind nginx, not a standalone Burxt server.** nginx owns TCP, TLS, HTTP parsing, static
files, timeouts and process management; Burxt owns the application. This is how PHP took the web,
and for a playground it is not a compromise — it deletes four rows from this plan.

A standalone `lib/http.bx` remains interesting, because a single self-contained binary is a real
Burxt selling point. It is **not** in this line, and the reason is written down so it is not re-argued:
nothing about the playground needs it, and writing a second-rate HTTP server to avoid a
first-rate one is how a language spends a release proving a point nobody asked about.

---

## The checklist

### P1 · `burxt effects` — the safety gate, and the thing nobody else has

**§Q1, specified since 1.0 and unbuilt.** `burxt effects <file.bx>` reports what a program can
reach, **with where each effect entered**, and `--allow <list>` exits non-zero when it reaches
anything outside the list.

| ☐ | # | Item |
|---|---|---|
| ☐ | P1a | `burxt effects <file>` — the effect set, and the call path that introduced each |
| ☐ | P1b | `--allow files,clock` — a gate, exit 70 with the offending path named |
| ☐ | P1c | `--json` for the playground to consume |
| ☐ | P1d | Stage-1 parity, or a refusal by name |

**The top level is exempt from effects** (§Q2, Andre's decision, verified v0.0.287), so this
cannot read a declaration off `region main`. It computes the union over everything reachable —
which is what Q1 asked for and is the harder, more useful answer.

**Why this is the first row.** Every other playground on the internet sandboxes at runtime and
hopes. Burxt can refuse a submission *before running it*, and the compiler guarantees the
declaration is complete — you cannot hide `system()` behind three wrappers. Nothing else can say
that, because nothing else makes a program declare its reach.

It is a gate for CI too: `burxt effects lib/decimal.bx --allow ""` is a test that the money layer
touches nothing, and it never goes stale.

### P2 · Bounding a stranger's program

| ☐ | # | Item |
|---|---|---|
| ☐ | P2a | `os_limit_cpu(seconds)` — `RLIMIT_CPU`, measured working, needs a name and a fixture |
| ☐ | P2b | `os_limit_memory(bytes)` — `RLIMIT_AS`, same shape, ten lines |
| ☐ | P2c | `os_limit_files(n)`, `os_limit_processes(n)` — `RLIMIT_NOFILE`, `RLIMIT_NPROC` |
| ☐ | P2d | Wall-clock kill in the parent, since `RLIMIT_CPU` does not catch a sleeping child |

**Stated plainly, because it would otherwise be assumed:** rlimits bound CPU, memory and handles.
They do **not** bound syscalls. A submission that honestly declares `touches commands` can still
run `curl`. P1 is what refuses it; rlimits are what survive it. **The deployment answer is a
container per submission**, and that is an operations problem this release does not pretend to
solve in the language.

### P3 · The view layer

Being built in a parallel session; listed so the release knows what it contains.

| ☐ | # | Item |
|---|---|---|
| ☐ | P3a | `lib/html.bx` — §W0's typed tree. Written, uncommitted |
| ☐ | P3b | `lib/cgi.bx` — request and response. Written, uncommitted |
| ☑ | P3c | BMX's implementation + the conformance suite. **Not in `lib/` — they are `burxt/bmx.bx` and `tests/` in github.com/andrecorugda/bmx**, reached as a package: `dependency bmx <repo> <commit>` and `use "bmx/burxt/bmx.bx"` |
| ☑ | P3d | Ruled, and not by default. BMX has its own repository, its own version — 0.2 against Burxt's 1.1 — and its own CI, which runs the format's suite with no Burxt installed **and** the Burxt implementation against a compiler built from source. A module in somebody else's standard library has that language's version and no say in its own; adding `Fenced` to the AST was a minor for the format and a major for anyone matching on it, and there was nowhere to record the difference |

### P3b · Exact money, all the way to the characters

Not a release of its own — it is a **patch**, because it adds no surface.

| ☐ | # | Item |
|---|---|---|
| ☐ | P3b | Render `Decimal` and `Int` to text **in Burxt**, not through the host's `snprintf`. Argument, measurement, cost and the fixture set: [`1.0/N1-BOUNDARY-EXACTNESS.md`](1.0/N1-BOUNDARY-EXACTNESS.md) §7 |

`codegen.rs:2360` builds `format!("%s%llu.%0{}llu", scale)` and hands it to whatever libc the
target has. **No conforming libc renders that differently**, so this is not a latent defect on any
platform Burxt ships to — the claim was drafted stronger than that and narrowed before landing.

The exposure is hosts that supply `printf` themselves, which is a surface that did not exist
before this month and is exactly where the language is going. A wasm island rendered **`$1299.05`
as `1299.5`** on 2026-08-16 because its varargs walker discarded the width — silently, no crash,
a factor of ten. Zero-padding is the one detail that corrupts money instead of crashing, and every
future host author is currently asked to get it right.

**The acceptance test is the version number.** It adds nothing, so it is a patch, so byte-identical
output against every fixture in §7.4 is the bar — plus stage-0 and stage-1 agreeing with each
other, because two implementations that both match glibc and not each other is a fixpoint failure
wearing a passing test.

**Fixtures before implementation.** Getting a zero-pad wrong reintroduces exactly this defect with
our name on it instead of a shim author's.

### P4 · The playground itself

| ☐ | # | Item |
|---|---|---|
| ☐ | P4a | `examples/playground/` — a real Burxt CGI app: accept source, gate on `burxt effects`, compile, run under limits, render the result through BMX |
| ☐ | P4b | The nginx configuration, in the repo, as the deployment record |
| ☐ | P4c | An end-to-end fixture: source in, output out, without a browser |

### P5 · Documentation, which is half the release

| ☐ | # | Item |
|---|---|---|
| ☐ | P5a | `docs/guide/` — a chapter on writing a server, CGI first |
| ☐ | P5b | `docs/reference/` — generated pages for every new module (the invariant already refuses a module without one) |
| ☐ | P5c | The version picker moves to **series**: "1.2 (latest) / 1.1 / 1.0" |
| ☐ | P5d | `docs/1.1/` frozen, the way `docs/1.0/` was, once 1.2 becomes latest |
| ☐ | P5e | `docs/limitations.md` — threads, TLS, DNS, and the syscall limit of rlimits |

---

## Verification

Unchanged from every release before it, and the two that bit hardest this month:

- **Both compilers, or it is not done.**
- **A fail fixture per refusal.** For P1 that means a program whose effects exceed `--allow` and
  exits non-zero with the path named — the passing case proves nothing on its own.
- **Any test that can block carries its own deadline.** `tests/pass/net_loopback.bx` wedged a CI
  runner for an hour, twice, before it called `alarm(20)` on itself.
- **A claim about another platform is measured on that platform.** Three defects this month were
  reasoned about correctly and were still wrong; a fifteen-minute `workflow_dispatch` probe
  settled each one.

## What 1.2 is NOT

- **Not threads.** C calling back into Burxt is the door, and it has not moved.
- **Not TLS.** Bound, never built — recorded at §E5. Terminate at the proxy.
- **Not DNS.** `getaddrinfo` hands back a pointer inside a chain of structs; reading a pointer out
  of C's memory is still shut.
- **Not a standalone HTTP server.** See the architecture note above.
- **Not a sandbox.** The language refuses and bounds; the container isolates.

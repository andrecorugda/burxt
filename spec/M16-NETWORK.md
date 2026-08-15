# M16 — Networking and concurrency: what the wall actually was

**Status: sockets DONE, processes DONE, threads not started.** Landed after 1.0.0.

---

## The claim this replaces

`spec/1.0/ROADMAP-1.0.md` §G2 read:

> **The pointer wall's remaining doors** (`M2`, four of them)
> - `c_bytes_at(p, n)` + a decision about what happens when `n` lies → `mmap`, N9 row 6
> - **Callbacks into Burxt** (C calling Burxt) → `sqlite3_exec`, most C libraries' iteration APIs
> - C → Burxt string returns …
> - An **effect for the environment** …
> - Then: sockets → TLS → HTTPS → `llama.cpp` FFI

Read as written, sockets sat behind four doors and a decision. `docs/limitations.md` said flatly
*"It cannot open a network connection."* `lib/os.bx` said *"Burxt can hold a pointer but cannot
build a struct behind one: `c_bytes_at` reads C's memory and nothing writes it"* — and chose the
obsolescent `usleep` over `nanosleep` because of it.

**None of that was measured. All of it was inherited.**

---

## What the measurement found

A program was written instead of a paragraph. Against **v1.0.0, with no compiler change at all**:

| | |
|---|---|
| `socket(2, 1, 0)` | returns a real fd |
| `listen(fd, 8)` | returns 0 — the kernel **auto-binds** an unbound TCP socket, so a server ran before `bind` did |
| `write(fd, text, n)` | **a Burxt String reaches C as `char *`**, so sending needed nothing |
| `accept(fd, NULL, NULL)` | works — and **NULL is obtainable**: `getenv` of an unset name |
| `read(fd, buf, n)` + `c_bytes_at` | receiving needed nothing |
| `fork()` / `waitpid()` | four workers, interleaved, all reaped |

A Burxt binary served an HTTP request to `nc`, then to `curl`, then from three pre-forked workers —
before a line of the compiler changed.

**The one genuine gap was `bind()` to a chosen port**: sixteen bytes of `struct sockaddr_in`, handed
over by pointer. Burxt could read C's memory and never write it.

So: **one builtin, not a milestone.** Seven doors were imagined; one was locked.

---

## `c_bytes_to(p, bytes) -> Int`

The exact mirror of `c_bytes_at`. Both compilers, v0.0.290.

- **The length is not a claim.** It is `len(bytes)`, which closes half of `c_bytes_at`'s soft edge:
  nothing can lie about how much is read *out of* Burxt. What stays the caller's claim is the
  destination's capacity, which belongs to C — the same bargain `as scaled` and `external function`
  already make.
- **A null destination exits 70**, the same as reading one.
- **An element outside `0..=255` exits 70 and does not mask.** `bit_and(x, 0xFF)` would write a byte
  that is not the number the caller wrote down; `256` quietly becoming `0` is a corrupt port, a
  corrupt length prefix, a corrupt checksum, each discovered somewhere else entirely. One branch is
  not a price worth taking for that.
- **No region needed** — it writes memory C already owns and answers a count.

Fixtures: `tests/pass/c_bytes_to.bx` (round trip), three refusals in `tests/fail/`, and
`tests/panic/c_bytes_to_refuses_a_non_byte.bx` for the range trap.

**One implementation note worth keeping.** Stage-1's array header is `[len, cap, data, width]`, and
the first draft of its IR helper read field 0 as the data pointer. It looked right. It would have
written the array's *length* into whatever the caller passed — a socket address, most likely — and
the fixture would have caught it only after the bytes were already wrong. Mirroring `@burxt.slot`'s
arithmetic rather than assuming the layout is what fixed it.

---

## `lib/net.bx`

TCP, and nothing above it. `net_listen`, `net_listen_any_port`, `net_port_of`, `net_accept`,
`net_connect_ipv4`, `net_read`, `net_write`, `net_close`.

Decisions worth not re-deriving:

- **Every fallible call answers `Option`.** A socket call returns -1, and -1 is a perfectly good
  descriptor as far as the type is concerned. `net_read` distinguishes `None` (error) from
  `Some("")` (the peer hung up) — folding those together would be `file_read`'s missing-file bug
  again, in a module written after that bug was fixed.
- **`net_write` loops**, because `send` may write less than it was given and routinely does on a
  slow peer. It also passes `MSG_NOSIGNAL`: writing to a closed socket raises SIGPIPE, which by
  default **kills the process**, and Burxt has no signal handlers to install instead.
- **`SO_REUSEADDR` is set**, so a restarted server does not fail on TIME_WAIT with an error that
  reads as "the port is taken" by something that is not there.
- **`write_sockaddr_in` writes rather than returns**, chosen by the region model rather than by
  taste — a function whose parameters are all `Int` carries no caller region and may not return
  `[Int]`. Same shape as `sha256_k`.
- **The port is big-endian and the family is not.** Written in one place, because a port written
  little-endian binds successfully to a different number, nothing fails, and a client finds it.
- **`net_connect_ipv4` names its limit**, the `string_to_upper_ascii` bargain. DNS needs
  `getaddrinfo`, which hands back a pointer buried in a chain of structs — reading a pointer *out*
  of C's memory is a different door from writing bytes *into* it, and it is still shut.

---

## `os_fork`, and a bug removed rather than documented

`print` goes through C's stdio, which is **fully buffered when stdout is a pipe** rather than a
terminal. `fork` copies the buffer, so anything printed and not yet flushed is printed again by
every child. A three-worker server printed its startup line four times — and only when redirected,
because a terminal is line-buffered.

`os_fork` calls `fflush(NULL)` before it splits. The caller never learns this was a problem, which
is the right amount for a caller to learn about it.

`os_wait_for_child` discards the exit status, and says so: reading it means `c_bytes_at` on four
bytes plus the `WIFEXITED` bit-twiddling C hides in macros. Reachable now; not written.

---

## Two stale limitations retired

Both had been **written down once, in prose, as the reason for a workaround, and then re-read as
fact** by everyone who came after.

1. **"Nothing in Burxt writes C's memory."** `lib/os.bx`'s header. It justified `usleep` over
   `nanosleep` and `uname`-by-subprocess over `uname(2)`. Both are now small jobs rather than walls.
   That header had *already been corrected once* for the same class of error — it previously claimed
   "anything returning a pointer is absent", false from v0.0.196.
2. **"A null pointer, which the language has no way to spell."** True of a *literal*; never true of
   the capability. `getenv` of an unset name is one, POSIX guarantees it, and `os_env` already
   depended on exactly that to tell "unset" from "empty".

---

## What is still shut

- **C calling back into Burxt.** This is the door that matters and it has not moved. It blocks
  `pthread_create` (hence threads), `sqlite3_exec`, `signal`, and most C iteration APIs.
- **Reading a pointer out of C's memory** — `c_pointer_at`, the third mirror. Blocks `getaddrinfo`,
  so DNS, and any API that answers a struct containing pointers.
- **TLS** — bound, not built. Recorded decision, §E5.
- **Timeouts / `poll`** — reachable today, not written.
- **Threads with derived mutual exclusion** — §G1, unchanged and still the interesting one.

---

## The rule this is the seventh instance of

A wall that looked like a design constraint dissolved without new machinery, again. The pattern is
now specific enough to state as a procedure:

> **When a document says something is impossible, the first move is to write the program, not to
> plan the milestone.** A limitation is evidence only on the day it is measured. This one had been
> re-read for months, cited in three files, and shaped two library functions around itself.

`docs/limitations.md` and `docs/comparison.md` were both wrong about networking on the day 1.0.0
shipped, and both were wrong in the direction that costs most: claiming an absence. Nobody re-tests
what does not work.

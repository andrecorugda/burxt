---
layout: doc
title: lib/os.bx
section: reference
description: "The machine the program is running on."
---

{% raw %}

# `lib/os.bx`

The machine the program is running on.

```burxt
use "lib/os.bx";
```

Arguments, the environment, the clock, the process itself, and running a command.

**This header has now been wrong about the same thing twice, which is worth more than either correction.** It first ended *"anything returning a pointer is absent"*, false from v0.0.196 when `getenv` and `getcwd` began returning `CPointer`. It was rewritten to say *"what is still absent is a struct behind a pointer — `uname(2)` fills one, `nanosleep` takes one — because `c_bytes_at` reads C's memory and nothing in Burxt writes it."* **That is false too, since `c_bytes_to`.** Something in Burxt writes C's memory now, and `lib/net.bx` builds a `sockaddr_in` with it.

So `os_platform` shelling out to `uname` and `os_sleep` calling `usleep` are no longer forced by the language. They are merely what is written, and each of those two is now a small job rather than a wall. A stale limitation is worse than a stale DONE, because nobody re-tests the thing that "doesn't work" — this file has proved that twice and gets one more sentence for it: **the reason a workaround exists stops being true before the workaround does.**

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Capture`](#capture) | class | Everything one command left behind: what it printed, what it complained about, and how it ended. |
| [`os_arg_count`](#os-arg-count) | function | The arguments the program was started with. Index 0 is the program's own path, as it is everywhere else. |
| [`os_arg`](#os-arg) | function | — |
| [`os_args`](#os-args) | function | Every argument after the program's own name. |
| [`os_now`](#os-now) | function | Seconds since 1970. Whole seconds, because that is what `time` answers — a finer clock needs `clock_gettime`, which fill |
| [`os_run`](#os-run) | function | Run a command through the shell and answer its exit code. |
| [`os_exit_code`](#os-exit-code) | function | A wait status turned into the number a shell would report. |
| [`os_capture_status`](#os-capture-status) | function | Run a command and answer what it printed **on standard output and standard error separately**, with its exit code. Roadm |
| [`os_capture`](#os-capture) | function | Run a command and answer what it printed, standard output and standard error **merged in the order the process wrote the |
| [`os_read_byte`](#os-read-byte) | function | One byte of standard input, or -1 at the end. The whole of the input, a byte at a time, is what a program can do today — |
| [`os_read_line`](#os-read-line) | function | One line of standard input, without its newline, or None at end of input. |
| [`os_read_all`](#os-read-all) | function | — |
| [`os_env`](#os-env) | function | An environment variable, or a stated absence. |
| [`os_env_or`](#os-env-or) | function | The same question with a stated fallback, for the common case where a missing setting has a sensible default. Separate f |
| [`os_set_env`](#os-set-env) | function | Set an environment variable for this process and every child it starts afterwards. |
| [`os_pid`](#os-pid) | function | The process's own id. |
| [`os_cwd`](#os-cwd) | function | The directory the process is running in, or a stated absence. |
| [`os_platform`](#os-platform) | function | Which operating system this is: `"linux"`, `"macos"`, or `uname -s` lowercased. |
| [`os_sleep`](#os-sleep) | function | Wait, and let the rest of the machine get on with it. Roadmap §D1r. |
| [`os_trim_ascii`](#os-trim-ascii) | function | Spaces, tabs, carriage returns and newlines off both ends. Local rather than `lib/string.bx`'s `string_trim`, for the re |
| [`os_is_space`](#os-is-space) | function | — |
| [`os_fork`](#os-fork) | function | Splits this process in two. Answers **0 in the child** and the child's pid in the parent. |
| [`os_wait_for_child`](#os-wait-for-child) | function | Waits for any child to finish and answers its pid, or -1 when there are none left. |
| [`os_flush`](#os-flush) | function | Empties every output buffer. Answers whether it worked. |
| [`os_env_missing`](#os-env-missing) | function | A null `CPointer`, for the C calls that want one. |
| [`os_rlimit_as`](#os-rlimit-as) | function | **The resource NUMBERS differ between Linux and the BSDs, and only `RLIMIT_CPU` agrees.** |
| [`os_rlimit_nofile`](#os-rlimit-nofile) | function | — |
| [`os_rlimit_nproc`](#os-rlimit-nproc) | function | — |
| [`os_set_limit`](#os-set-limit) | function | `struct rlimit { rlim_t rlim_cur; rlim_t rlim_max; }` — two 64-bit values, sixteen bytes, handed over by pointer. Writab |
| [`os_limit_cpu`](#os-limit-cpu) | function | **CPU seconds. The one limit whose resource number is the same everywhere.** |
| [`os_limit_memory`](#os-limit-memory) | function | Address space, in bytes. Past it, `malloc` answers null rather than the kernel killing anything. |
| [`os_limit_files`](#os-limit-files) | function | Open file descriptors, and child processes. Both bound what a runaway can take from the machine rather than from itself  |
| [`os_limit_processes`](#os-limit-processes) | function | — |
| [`os_die_after`](#os-die-after) | function | **Wall-clock, which is the one `RLIMIT_CPU` cannot do.** SIGALRM's default action ends the process, so this is a hard ce |

## Types
{: #types}

### `Capture`
{: #capture}

```burxt
class Capture { output: String, errors: String, code: Int }
```

Everything one command left behind: what it printed, what it complained about, and how it ended.

Three fields rather than three functions, because running the command three times to ask three questions would run it three times — and a command with an effect is not a question you may ask twice.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L127)

## Functions
{: #functions}

### `os_arg_count`
{: #os-arg-count}

```burxt
function os_arg_count() -> Int touches input
```

The arguments the program was started with. Index 0 is the program's own path, as it is everywhere else.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L70)

### `os_arg`
{: #os-arg}

```burxt
function os_arg(index: Int) -> String touches input
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L74)

### `os_args`
{: #os-args}

```burxt
function os_args() -> [String] touches input
```

Every argument after the program's own name.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L79)

### `os_now`
{: #os-now}

```burxt
function os_now() -> Int touches clock
```

Seconds since 1970. Whole seconds, because that is what `time` answers — a finer clock needs `clock_gettime`, which fills a class through a pointer, and that is exactly what Burxt will not let C do yet.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L92)

### `os_run`
{: #os-run}

```burxt
function os_run(command: String) -> Int touches commands
```

Run a command through the shell and answer its exit code.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L97)

### `os_exit_code`
{: #os-exit-code}

```burxt
pure function os_exit_code(status: Int) -> Int
```

A wait status turned into the number a shell would report.

**This used to be `divide_floor(status, 256)` and that lost a whole category of failure.** A command killed by a signal has an exit status of zero in its high byte, so a program terminated by SIGSEGV or SIGKILL — or by the timeout that was supposed to bound it — reported **success**. The low seven bits are the signal, and `sh` reports `128 + signal` for exactly this reason, so that is what this answers: 137 for SIGKILL, 139 for a segfault.

`-1` when the command did not run at all, which is what `system` answers when the fork failed. It is the one value no exited process can produce.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L111)

### `os_capture_status`
{: #os-capture-status}

```burxt
function os_capture_status(command: String) -> Capture touches commands, files, input
```

Run a command and answer what it printed **on standard output and standard error separately**, with its exit code. Roadmap §D1q.

`os_capture` below merges the two with `2>&1`, and merging is destructive: a caller that wants the output of `git rev-parse` cannot tell the hash from the warning that came with it, and a caller checking whether anything went wrong has nothing to check. Both streams get their own file here, in a directory nobody else can enter.

**What is lost by separating them is the interleaving.** `2>&1` puts both streams in one file in the order the process wrote them; two files cannot say which line came first. That is the trade, and `os_capture` is still the right call when the order is the thing you want to see.

A command that could not be run at all — no private scratch directory — answers empty strings and `-1`, the same code `os_run` uses for a command that never started.

**The command is wrapped in `( ... )`, and `os_capture` had been silently wrong for want of it.** A redirection binds to ONE command, so `sh -c "echo a; echo b > f"` writes only `b` to the file and prints `a` to the terminal the program is running in. Every captured command containing a `;`, a `&&` or a `||` — which is most of the interesting ones — had all but its last piece escape the capture and land on the caller's own output. Found by giving this function a two-statement command in its first test, which is the case nobody had written.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L150)

### `os_capture`
{: #os-capture}

```burxt
function os_capture(command: String) -> String touches commands, files, input
```

Run a command and answer what it printed, standard output and standard error **merged in the order the process wrote them**. The output travels through a file because a pipe would mean `popen`, which answers a pointer whose lifetime Burxt would have to reason about.

Kept beside `os_capture_status` rather than replaced by it: interleaving is a real thing to want, and it is the half two separate files cannot reconstruct. What this cannot tell you is whether the command succeeded — `os_capture_status` is the one to reach for then.

§B3: the scratch path used to be the constant `/tmp/burxt-os-capture`. Two copies of a program overwrote each other's output, and anyone on the machine could leave a symlink on that name for the redirect to truncate. It is now a fresh 0700 directory per call.

The `( ... )` is not decoration — see `os_capture_status` above for the capture this function was losing without it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L187)

### `os_read_byte`
{: #os-read-byte}

```burxt
function os_read_byte() -> Int touches input
```

One byte of standard input, or -1 at the end. The whole of the input, a byte at a time, is what a program can do today — `fgets` needs a buffer it does not own.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L203)

### `os_read_line`
{: #os-read-line}

```burxt
function os_read_line() -> Option<String> touches input
```

One line of standard input, without its newline, or None at end of input.

The distinction from `os_read_all` is the whole reason this exists: `os_read_all` blocks until EOF, which never arrives for a SERVER. A protocol that frames its messages one per line — MCP over stdio, and most of the others — needs to answer the first request before the client has sent the second, so reading to EOF is not a slow version of this, it is a deadlock.

A bare `\r` before the newline is dropped, so a CRLF client and an LF client are read identically. Nothing else is stripped: a line is its bytes.

`None` at end of input rather than an empty String, because an empty LINE is a real thing a client can send and the two must be distinguishable. That is the same reason `string_parse_int` exists beside `string_to_int`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L220)

### `os_read_all`
{: #os-read-all}

```burxt
function os_read_all() -> String touches input
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L250)

### `os_env`
{: #os-env}

```burxt
function os_env(name: String) -> Option<String> touches input
```

An environment variable, or a stated absence.

`Option<String>` and not `String`, because **unset and empty are different facts.** `FOO=` sets FOO to the empty string; not mentioning FOO at all is a different thing, and a library that answered "" for both would make "is this configured" unanswerable. `getenv` distinguishes them by returning NULL, and this is where that distinction is preserved rather than flattened.

`touches input` because the value came from outside the program. Whether reading the environment deserves an effect of its own is an open question — see spec/FAR-HORIZON-ROADMAP.md M2 — but `input` is honest today: it is a value the process was started with.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L278)

### `os_env_or`
{: #os-env-or}

```burxt
function os_env_or(name: String, fallback: String) -> String touches input
```

The same question with a stated fallback, for the common case where a missing setting has a sensible default. Separate from `os_env` rather than a parameter with a default, because Burxt has no default arguments — and because the two really are different questions.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L290)

### `os_set_env`
{: #os-set-env}

```burxt
function os_set_env(name: String, value: String) -> Bool touches input
```

Set an environment variable for this process and every child it starts afterwards.

**Overwrites**, always. `setenv`'s third argument chooses, and a library that exposed the choice would make `os_set_env` a question rather than an instruction; a caller who wants "only if unset" has `os_env` right above and can say so in a line a reviewer can read.

`false` when the name is unusable — empty, or containing `=`, which is `setenv`'s own refusal — or when the allocation for the copy failed. It does **not** change the environment of the shell that started the program: no process can do that, and it is worth saying because it is the first thing people expect this to do.

`touches input` and not an effect of its own. The environment is process state that Burxt has no effect for, and `input` is the one that already names it — `os_env` reads through the same effect, and reading and writing the same place should not be filed apart. Whether process state deserves its own effect is spec/FAR-HORIZON-ROADMAP.md M2, the same open question `os_env` points at.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L310)

### `os_pid`
{: #os-pid}

```burxt
function os_pid() -> Int touches input
```

The process's own id.

Small, and reused by the kernel once the process is gone, so it is a fine way to keep two concurrent programs from choosing the same scratch name and **not** a secret. `lib/files.bx` puts it in a temp directory's name for the first reason and relies on `mkdir` for the second — see §B3 there, which is the row this function closed.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L320)

### `os_cwd`
{: #os-cwd}

```burxt
function os_cwd() -> Option<String> touches files
```

The directory the process is running in, or a stated absence.

`None` when the path does not fit in `OS_PATH_MAX`, or when the directory has been deleted out from under the process — a real thing that happens to long-running programs, and the reason this answers `Option` rather than `""`.

The buffer is this function's: `getcwd` fills memory the caller owns, so nothing here has to free something C allocated, and `c_string_at` copies the bytes out before the `free`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L332)

### `os_platform`
{: #os-platform}

```burxt
function os_platform() -> String touches commands, files, input
```

Which operating system this is: `"linux"`, `"macos"`, or `uname -s` lowercased.

**It shells out**, one `uname` per call, and a caller asking in a loop should ask once. There is no cheaper answer available: nothing in the language exposes the target it was compiled for, and `uname(2)` fills a struct through a pointer, which is the wall Burxt still has.

Two decisions worth stating. `darwin` comes back as `"macos"`, because that is the name the roadmap, CI and every caller use, and a library that made everyone remember the kernel's name would be pedantry with a cost. **Everything else comes back as what the machine said**, in lower case, rather than `"unknown"` — a FreeBSD box answering `"freebsd"` is legible, and answering `"unknown"` throws away the one fact that was available.

Empty when `uname` is not on the PATH, which is a stated failure rather than a guess.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L360)

### `os_sleep`
{: #os-sleep}

```burxt
function os_sleep(milliseconds: Int) -> Bool touches clock
```

Wait, and let the rest of the machine get on with it. Roadmap §D1r.

Every retry loop, every poll of a file that another process is writing, and every backoff in this language spun the CPU flat out before this existed, because there was no way to yield.

**`usleep` and not `nanosleep`, and the reason is the C boundary rather than taste.** `nanosleep(const struct timespec *, struct timespec *)` takes a two-field struct BY POINTER, and Burxt can hold a pointer but cannot build a struct behind one: `c_bytes_at` reads C's memory and nothing writes it. `poll(NULL, 0, ms)` is the other usual trick and needs a null pointer literal, which the language has no way to spell. `sleep(3)` takes whole seconds, which is not what a poll loop needs. That leaves `usleep`, which is obsolescent in POSIX.1-2008 and present everywhere in practice — and if it ever is not, this is the one function that changes.

`usleep`'s argument must be **under one million**, so a longer wait is sliced into 900 ms pieces. That is the loop below, and it is the whole of it.

**A signal cuts the wait short and answers `false`**, having slept less than asked. It is not retried, because "sleep at least this long" and "return when something happened" are different intents and only the caller knows which one it had.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L394)

### `os_trim_ascii`
{: #os-trim-ascii}

```burxt
function os_trim_ascii(text: String) -> String
```

Spaces, tabs, carriage returns and newlines off both ends. Local rather than `lib/string.bx`'s `string_trim`, for the reason the whole of this file is: `use "lib/os.bx"` should pull in the operating system and not two thousand lines of string handling.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L414)

### `os_is_space`
{: #os-is-space}

```burxt
function os_is_space(b: Int) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L427)

### `os_fork`
{: #os-fork}

```burxt
function os_fork() -> Int touches commands, input
```

Splits this process in two. Answers **0 in the child** and the child's pid in the parent.

**It flushes first, and that is not tidiness — it is a correctness bug removed rather than documented.** `print` goes through C's stdio, which is fully buffered when stdout is a pipe or a file rather than a terminal. `fork` copies the process *including that buffer*, so anything printed and not yet flushed is printed again by every child. A pre-forked server that announced "listening on 18080" once printed it four times with three workers — on a terminal it looked perfect, because a terminal is line-buffered, and it only misbehaved when the output was redirected. That is the shape of defect this language exists to refuse, so `os_fork` empties the buffer before it splits and the caller never has to know.

`fflush(NULL)` flushes every open stream, which is what a fork wants — stdout and stderr both. The null pointer comes from `os_env_missing`, because the language has no literal for one.

**The child must not fall out of the bottom of the caller's loop.** A child that keeps looping forks again, and a program that forks in a loop it never leaves is a fork bomb. Every use of this looks like `if os_fork() == 0 { ...work...; return; }`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L479)

### `os_wait_for_child`
{: #os-wait-for-child}

```burxt
function os_wait_for_child() -> Int touches commands, input
```

Waits for any child to finish and answers its pid, or -1 when there are none left.

The exit STATUS is discarded, and that is a limit with a name rather than an oversight: `waitpid` reports it by filling an `int` the caller supplies, and reading it back means `c_bytes_at` on four bytes plus the `WIFEXITED`/`WEXITSTATUS` bit-twiddling that C hides in macros. Reachable now that `c_bytes_to` exists — every piece is here — and not yet written. `os_wait_for_child_status` is what it would be called.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L491)

### `os_flush`
{: #os-flush}

```burxt
function os_flush() -> Bool touches input
```

Empties every output buffer. Answers whether it worked.

`print` is buffered, so output written just before a crash, a `fork` or an `exec` can be lost or duplicated. `os_fork` calls this for you; a program that hands its stdout to something else mid-run wants it directly.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L500)

### `os_env_missing`
{: #os-env-missing}

```burxt
function os_env_missing() -> CPointer touches input
```

A null `CPointer`, for the C calls that want one.

`getenv` of a name nothing sets answers NULL — POSIX guarantees it, and `os_env` above already depends on exactly that to tell "unset" from "empty". It reads as a trick and it is the only spelling the language has: `CPointer` has no literal, deliberately, because a literal address is the one thing the pointer wall exists to refuse.

**The header above is the reason this is worth a paragraph.** The absence of a null pointer was written down as a fact and used to justify choosing `usleep` over `nanosleep`. It was never a fact about the language, only about the syntax, and nobody tried the four-line workaround for long enough that it became load-bearing.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L515)

### `os_rlimit_as`
{: #os-rlimit-as}

```burxt
function os_rlimit_as() -> Int touches input
```

**The resource NUMBERS differ between Linux and the BSDs, and only `RLIMIT_CPU` agrees.**

```burxt
            Linux   macOS/BSD
 CPU          0         0      <- the only one that matches
 FSIZE        1         1
 DATA         2         2
 STACK        3         3
 CORE         4         4
 AS           9         5
 NPROC        6         7
 NOFILE       7         8
```

Measured from `bits/resource.h` on this machine and from the BSD header, because this is the third time in one week that a small C struct or constant has turned out to differ by platform and the first two were reasoned about wrongly. `lib/net.bx`'s `sockaddr_in` is the same shape: a layout that is obvious, identical-looking, and not the same.

**So the numbering is asked of the kernel rather than assumed.** `RLIM_NLIMITS` is 16 on Linux and 9 on the BSDs, so resource 9 is valid on Linux and out of range everywhere else — `getrlimit(9, ...)` succeeding is the kernel saying "Linux numbering" in its own words. That is a positive answer rather than an inference from a failure, which is the distinction that cost a CI runner two hours when `net_uses_bsd_sockaddr` asked the question the other way round.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L549)

### `os_rlimit_nofile`
{: #os-rlimit-nofile}

```burxt
function os_rlimit_nofile() -> Int touches input
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L559)

### `os_rlimit_nproc`
{: #os-rlimit-nproc}

```burxt
function os_rlimit_nproc() -> Int touches input
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L569)

### `os_set_limit`
{: #os-set-limit}

```burxt
function os_set_limit(resource: Int, value: Int) -> Bool touches commands
```

`struct rlimit { rlim_t rlim_cur; rlim_t rlim_max; }` — two 64-bit values, sixteen bytes, handed over by pointer. Writable since `c_bytes_to`; before it, none of this file existed.

Both fields are set to the same value, which is deliberate: raising a limit later needs privilege a sandboxed child does not have and should not be given. A limit you can undo is not a limit.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L585)

### `os_limit_cpu`
{: #os-limit-cpu}

```burxt
function os_limit_cpu(seconds: Int) -> Bool touches commands
```

**CPU seconds. The one limit whose resource number is the same everywhere.**

The kernel sends SIGXCPU at the soft limit and SIGKILL at the hard one; both are set here, so a program that ignores the first does not get to ignore the second. It counts CPU time, not wall-clock — a child that sleeps for an hour spends no CPU and this will not stop it. That is what `os_die_after` is for, and the two are not alternatives.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L612)

### `os_limit_memory`
{: #os-limit-memory}

```burxt
function os_limit_memory(bytes: Int) -> Bool touches commands, input
```

Address space, in bytes. Past it, `malloc` answers null rather than the kernel killing anything.

**This is the one that bounds a Burxt program's arena**, which reserves its region up front — so a limit below the reservation means the program fails to start rather than failing partway, and that is the better failure.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L623)

### `os_limit_files`
{: #os-limit-files}

```burxt
function os_limit_files(count: Int) -> Bool touches commands, input
```

Open file descriptors, and child processes. Both bound what a runaway can take from the machine rather than from itself — a program that cannot fork cannot fork-bomb.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L631)

### `os_limit_processes`
{: #os-limit-processes}

```burxt
function os_limit_processes(count: Int) -> Bool touches commands, input
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L637)

### `os_die_after`
{: #os-die-after}

```burxt
function os_die_after(seconds: Int) -> Int touches clock
```

**Wall-clock, which is the one `RLIMIT_CPU` cannot do.** SIGALRM's default action ends the process, so this is a hard ceiling on elapsed time whether the program is computing, sleeping, or blocked on a socket that will never answer.

The timer does NOT survive `fork`, so a child that needs one must set its own. That is not a wart to work around — it is what lets a pre-forked server give each worker its own deadline.

`tests/pass/net_loopback.bx` calls this on itself for exactly that reason, after blocking a CI runner for a full hour twice.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L652)


{% endraw %}

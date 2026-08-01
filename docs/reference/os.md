---
layout: doc
title: lib/os.bx
section: reference
description: The machine the program is running on.
---


# `lib/os.bx`

The machine the program is running on.

```burxt
use "lib/os.bx";
```

Arguments, the clock, running a command, and exiting with a code. Each one wraps a C function whose signature Burxt can actually describe — ints and strings in, an int out. Anything returning a pointer is absent, and the guide's chapter on the C boundary says why.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`os_arg_count`](#os-arg-count) | function | The arguments the program was started with. Index 0 is the program's own path, as it is everywhere else. |
| [`os_arg`](#os-arg) | function | — |
| [`os_args`](#os-args) | function | Every argument after the program's own name. |
| [`os_now`](#os-now) | function | Seconds since 1970. Whole seconds, because that is what `time` answers — a finer clock needs `clock_gettime`, which fill |
| [`os_run`](#os-run) | function | Run a command through the shell and answer its exit code. `system` reports a wait status; the exit code is its high byte |
| [`os_capture`](#os-capture) | function | Run a command and answer what it printed. The output travels through a file because a pipe would mean `popen`, which ans |
| [`os_read_byte`](#os-read-byte) | function | One byte of standard input, or -1 at the end. The whole of the input, a byte at a time, is what a program can do today — |
| [`os_read_line`](#os-read-line) | function | One line of standard input, without its newline, or None at end of input. |
| [`os_read_all`](#os-read-all) | function | — |
| [`os_env`](#os-env) | function | An environment variable, or a stated absence. |
| [`os_env_or`](#os-env-or) | function | The same question with a stated fallback, for the common case where a missing setting has a sensible default. Separate f |

## Functions
{: #functions}

### `os_arg_count`
{: #os-arg-count}

```burxt
function os_arg_count() -> Int touches input
```

The arguments the program was started with. Index 0 is the program's own path, as it is everywhere else.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L23)

### `os_arg`
{: #os-arg}

```burxt
function os_arg(index: Int) -> String touches input
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L27)

### `os_args`
{: #os-args}

```burxt
function os_args() -> [String] touches input
```

Every argument after the program's own name.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L32)

### `os_now`
{: #os-now}

```burxt
function os_now() -> Int touches clock
```

Seconds since 1970. Whole seconds, because that is what `time` answers — a finer clock needs `clock_gettime`, which fills a class through a pointer, and that is exactly what Burxt will not let C do yet.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L45)

### `os_run`
{: #os-run}

```burxt
function os_run(command: String) -> Int touches commands
```

Run a command through the shell and answer its exit code. `system` reports a wait status; the exit code is its high byte.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L51)

### `os_capture`
{: #os-capture}

```burxt
function os_capture(command: String) -> String touches commands, files
```

Run a command and answer what it printed. The output travels through a file because a pipe would mean `popen`, which answers a pointer.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L57)

### `os_read_byte`
{: #os-read-byte}

```burxt
function os_read_byte() -> Int touches input
```

One byte of standard input, or -1 at the end. The whole of the input, a byte at a time, is what a program can do today — `fgets` needs a buffer it does not own.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L65)

### `os_read_line`
{: #os-read-line}

```burxt
function os_read_line() -> Option<String> touches input
```

One line of standard input, without its newline, or None at end of input.

The distinction from `os_read_all` is the whole reason this exists: `os_read_all` blocks until EOF, which never arrives for a SERVER. A protocol that frames its messages one per line — MCP over stdio, and most of the others — needs to answer the first request before the client has sent the second, so reading to EOF is not a slow version of this, it is a deadlock.

A bare `\r` before the newline is dropped, so a CRLF client and an LF client are read identically. Nothing else is stripped: a line is its bytes.

`None` at end of input rather than an empty String, because an empty LINE is a real thing a client can send and the two must be distinguishable. That is the same reason `string_parse_int` exists beside `string_to_int`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L82)

### `os_read_all`
{: #os-read-all}

```burxt
function os_read_all() -> String touches input
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L112)

### `os_env`
{: #os-env}

```burxt
function os_env(name: String) -> Option<String> touches input
```

An environment variable, or a stated absence.

`Option<String>` and not `String`, because **unset and empty are different facts.** `FOO=` sets FOO to the empty string; not mentioning FOO at all is a different thing, and a library that answered "" for both would make "is this configured" unanswerable. `getenv` distinguishes them by returning NULL, and this is where that distinction is preserved rather than flattened.

`touches input` because the value came from outside the program. Whether reading the environment deserves an effect of its own is an open question — see spec/FAR-HORIZON-ROADMAP.md M2 — but `input` is honest today: it is a value the process was started with.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L140)

### `os_env_or`
{: #os-env-or}

```burxt
function os_env_or(name: String, fallback: String) -> String touches input
```

The same question with a stated fallback, for the common case where a missing setting has a sensible default. Separate from `os_env` rather than a parameter with a default, because Burxt has no default arguments — and because the two really are different questions.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L152)


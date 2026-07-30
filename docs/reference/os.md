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
| [`os_byte_as_string`](#os-byte-as-string) | function | A single byte as a one-character String. Burxt has no character type and `to_string` of an Int gives digits, so the only |

## Functions
{: #functions}

### `os_arg_count`
{: #os-arg-count}

```burxt
function os_arg_count() -> Int touches input
```

The arguments the program was started with. Index 0 is the program's own path, as it is everywhere else.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L20)

### `os_arg`
{: #os-arg}

```burxt
function os_arg(index: Int) -> String touches input
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L24)

### `os_args`
{: #os-args}

```burxt
function os_args() -> [String] touches input
```

Every argument after the program's own name.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L29)

### `os_now`
{: #os-now}

```burxt
function os_now() -> Int touches clock
```

Seconds since 1970. Whole seconds, because that is what `time` answers — a finer clock needs `clock_gettime`, which fills a class through a pointer, and that is exactly what Burxt will not let C do yet.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L42)

### `os_run`
{: #os-run}

```burxt
function os_run(command: String) -> Int touches commands
```

Run a command through the shell and answer its exit code. `system` reports a wait status; the exit code is its high byte.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L48)

### `os_capture`
{: #os-capture}

```burxt
function os_capture(command: String) -> String touches commands, files
```

Run a command and answer what it printed. The output travels through a file because a pipe would mean `popen`, which answers a pointer.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L54)

### `os_read_byte`
{: #os-read-byte}

```burxt
function os_read_byte() -> Int touches input
```

One byte of standard input, or -1 at the end. The whole of the input, a byte at a time, is what a program can do today — `fgets` needs a buffer it does not own.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L62)

### `os_read_line`
{: #os-read-line}

```burxt
function os_read_line() -> Option<String> touches input
```

One line of standard input, without its newline, or None at end of input.

The distinction from `os_read_all` is the whole reason this exists: `os_read_all` blocks until EOF, which never arrives for a SERVER. A protocol that frames its messages one per line — MCP over stdio, and most of the others — needs to answer the first request before the client has sent the second, so reading to EOF is not a slow version of this, it is a deadlock.

A bare `\r` before the newline is dropped, so a CRLF client and an LF client are read identically. Nothing else is stripped: a line is its bytes.

`None` at end of input rather than an empty String, because an empty LINE is a real thing a client can send and the two must be distinguishable. That is the same reason `string_parse_int` exists beside `string_to_int`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L79)

### `os_read_all`
{: #os-read-all}

```burxt
function os_read_all() -> String touches input
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L106)

### `os_byte_as_string`
{: #os-byte-as-string}

```burxt
function os_byte_as_string(byte: Int) -> String
```

A single byte as a one-character String. Burxt has no character type and `to_string` of an Int gives digits, so the only way through is a table.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/os.bx#L126)


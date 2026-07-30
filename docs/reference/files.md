---
layout: doc
title: lib/files.bx
section: reference
description: Files, without writing `external function fopen` yourself.
---


# `lib/files.bx`

Files, without writing `external function fopen` yourself.

```burxt
use "lib/files.bx";
```

The language gives `read_file` and `write_file`, which read and replace a whole file. Everything else a program needs from a filesystem — appending, existence, listing a directory, deleting — is here, and each one is written ONCE so that nobody has to promise the compiler something it cannot check.

That promise is the reason this file exists. `opendir` returns a pointer, and Burxt refuses a C return whose ownership it cannot describe (see the guide's chapter on the C boundary), so listing a directory is not directly expressible. The honest way through is the shell, and it belongs behind one function rather than in every program.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`file_read`](#file-read) | function | Everything a path holds, as one String. Empty when the file cannot be read — which is indistinguishable from an empty fi |
| [`file_write`](#file-write) | function | Replaces the file. Answers the number of bytes written. |
| [`file_append`](#file-append) | function | Adds to the end, leaving what was there. Read-modify-write, because the builtin replaces rather than appends — fine for  |
| [`file_exists`](#file-exists) | function | — |
| [`file_delete`](#file-delete) | function | — |
| [`file_move`](#file-move) | function | — |
| [`file_make_directory`](#file-make-directory) | function | 0755: readable and executable by everyone, writable by its owner. |
| [`file_list_directory`](#file-list-directory) | function | Every name in a directory. The shell does the reading because `opendir` returns a pointer Burxt will not accept, and the |
| [`file_quote`](#file-quote) | function | A path made safe for the shell. Single quotes stop every expansion there is, and a single quote inside a path is the one |
| [`file_split_lines`](#file-split-lines) | function | Split on newlines, dropping the empty piece a trailing newline leaves. Local rather than borrowed from lib/string.bx, so |

## Functions
{: #functions}

### `file_read`
{: #file-read}

```burxt
function file_read(path: String) -> String touches files
```

Everything a path holds, as one String. Empty when the file cannot be read — which is indistinguishable from an empty file, and the honest fix is `Option<String>`, which the language does not have yet. Said plainly rather than papered over.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L27)

### `file_write`
{: #file-write}

```burxt
function file_write(path: String, text: String) -> Int touches files
```

Replaces the file. Answers the number of bytes written.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L32)

### `file_append`
{: #file-append}

```burxt
function file_append(path: String, text: String) -> Int touches files
```

Adds to the end, leaving what was there. Read-modify-write, because the builtin replaces rather than appends — fine for a log line, wrong for a gigabyte, and that is worth knowing before you use it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L39)

### `file_exists`
{: #file-exists}

```burxt
function file_exists(path: String) -> Bool touches commands
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L44)

### `file_delete`
{: #file-delete}

```burxt
function file_delete(path: String) -> Bool touches files
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L50)

### `file_move`
{: #file-move}

```burxt
function file_move(from: String, to: String) -> Bool touches files
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L54)

### `file_make_directory`
{: #file-make-directory}

```burxt
function file_make_directory(path: String) -> Bool touches commands
```

0755: readable and executable by everyone, writable by its owner.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L59)

### `file_list_directory`
{: #file-list-directory}

```burxt
function file_list_directory(directory: String) -> [String] touches commands, files
```

Every name in a directory. The shell does the reading because `opendir` returns a pointer Burxt will not accept, and the answer comes back through a file — visible here, so no program has to know it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L66)

### `file_quote`
{: #file-quote}

```burxt
function file_quote(path: String) -> String
```

A path made safe for the shell. Single quotes stop every expansion there is, and a single quote inside a path is the one case they cannot hold — so it is refused rather than mishandled.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L77)

### `file_split_lines`
{: #file-split-lines}

```burxt
function file_split_lines(text: String) -> [String]
```

Split on newlines, dropping the empty piece a trailing newline leaves. Local rather than borrowed from lib/string.bx, so `use "lib/files.bx"` pulls in files and nothing else.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L92)


---
layout: doc
title: lib/files.bx
section: reference
description: "Files, without writing `external function fopen` yourself."
---

{% raw %}

# `lib/files.bx`

Files, without writing `external function fopen` yourself.

```burxt
use "lib/files.bx";
```

The language gives `read_file` and `write_file`, which read and replace a whole file. Everything else a program needs from a filesystem — appending, existence, listing a directory, deleting — is here, and each one is written ONCE so that nobody has to promise the compiler something it cannot check.

---- the header used to say the opposite of what is true now ---------------------------

It read: *"`opendir` returns a pointer, and Burxt refuses a C return whose ownership it cannot describe, so listing a directory is not directly expressible."* **The pointer wall opened in v0.0.196** — `-> CPointer` is legal, `c_is_null` and `c_string_at` read one, and `c_bytes_at` (§A1) reads a counted buffer. That paragraph had been stale for eighty versions, and it was the stated reason every question here went through the shell.

So the mechanisms are now chosen one at a time rather than by a blanket rule:

* `file_is_directory` calls **`opendir`** — one syscall, no fork, and no quoting. That matters

```burxt
 because it is the question asked once per entry by anything walking a tree.
```

* `file_read_maybe`, `file_read_bytes` and `file_size` call **`fopen`**, so the answer to

```burxt
 "is it there" comes from the same open that reads it.
```

* Listing still goes through the **shell**, and this is the one place the old reasoning

```burxt
 survives: `readdir` answers a `struct dirent *`, and reading `d_name` out of it means
 knowing a field offset that differs between Linux and macOS. A pointer Burxt can hold is not
 a struct Burxt can read.
```

---- absence is a type here, and that closes §B1 — which was worse than it said -----------

§B1 reads: *"`file_read` of a missing file answers `""` — indistinguishable from an empty file."* **That is not what it does, and this file's own comment said the same wrong thing.** Measured, on the path `/tmp/definitely-not-here`:

```burxt
 burxt runtime error: cannot open file for reading
 exit 70
```

`read_file` does not answer anything. It **ends the process**, and `lib/burxt-compiler/lsp.bx` already knew — it refuses to use the module loader for exactly this reason, because `use "not-written-yet.bx";` is something people type every day and a language server that dies there is the worst failure available. So the bug was never a silent wrong answer; it was that **there was no way to read a file that might not be there.** Every caller either knew the file existed or accepted that a missing one killed the program.

A DIRECTORY is worse again, and nobody had written that case down. `fopen` on a directory succeeds, `fseek` to its end answers 9223372036854775807, and `read_file` then tries to allocate that:

```burxt
 burxt runtime error: region memory exhausted — this build reserves 4 GB per process
```

which blames memory for what is a directory handed to a file reader. That message is the runtime's and not this file's to fix — it is reported as its own row — but `file_read_maybe` and everything beside it rule the case out before `read_file` can see it.

**`file_read_maybe` is §B1's fix**, and it is a bigger fix than the row asked for: `None` for missing, `None` for unreadable, `None` for a directory, `Some("")` for a file that is genuinely empty, and in none of those cases does the program stop. `file_read` stays for the caller who already knows the file is there.

The same shape governs everything since: `file_size`, `file_read_bytes`, `file_walk` and — as of §B51, v0.0.288 — `file_list_directory` all answer `Option`, because "no such file" and "zero bytes" and "empty directory" are different facts. That last one held out longest, defended as published API, and it was the only function in this module that could still answer a caller's question wrongly without saying so.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`file_read`](#file-read) | function | Everything a path holds, as one String. |
| [`file_read_maybe`](#file-read-maybe) | function | Everything a path holds, or a stated absence. **This is roadmap §B1's fix.** |
| [`file_read_bytes`](#file-read-bytes) | function | Every byte of a file, as numbers 0..255, or a stated absence. |
| [`file_size`](#file-size) | function | How many bytes a file holds, or a stated absence. |
| [`file_write`](#file-write) | function | Replaces the file. Answers the number of bytes written. |
| [`file_append`](#file-append) | function | Adds to the end, leaving what was there. Read-modify-write, because the builtin replaces rather than appends — fine for  |
| [`file_exists`](#file-exists) | function | — |
| [`file_is_directory`](#file-is-directory) | function | Whether the path is a directory. |
| [`file_is_file`](#file-is-file) | function | Whether the path exists and is not a directory. |
| [`file_delete`](#file-delete) | function | — |
| [`file_move`](#file-move) | function | — |
| [`file_copy`](#file-copy) | function | Copies a file's contents and mode. **Not a directory** — `cp` without `-R` refuses one, and the refusal arrives here as  |
| [`file_make_directory`](#file-make-directory) | function | 0755: readable and executable by everyone, writable by its owner. |
| [`file_remove_directory`](#file-remove-directory) | function | Removes an **empty** directory. |
| [`file_list_directory`](#file-list-directory) | function | Every name in a directory, or `None` if there is no such directory. §B51. |
| [`file_walk`](#file-walk) | function | Every path under a directory, recursively, sorted — or `None` if there is no such directory. |
| [`file_temp_directory`](#file-temp-directory) | function | A private directory, made atomically, or a stated failure. |
| [`file_temp_path`](#file-temp-path) | function | A path for a temporary file, inside a fresh private directory. Roadmap §D1m's `temp_file`. |
| [`file_temp_release`](#file-temp-release) | function | Deletes a path from `file_temp_path` **and the private directory holding it**. |
| [`file_temp_base`](#file-temp-base) | function | `TMPDIR` if the environment sets one, `/tmp` otherwise — the convention every POSIX tool follows, and the reason a build |
| [`file_quote`](#file-quote) | function | A path made safe for the shell. Single quotes stop every expansion there is, and a single quote inside a path is the one |
| [`file_split_lines`](#file-split-lines) | function | Split on newlines, dropping the empty piece a trailing newline leaves. Local rather than borrowed from lib/string.bx, so |
| [`file_last_slash`](#file-last-slash) | function | The last `/`, or -1. Local for the same reason `file_split_lines` is: `lib/path.bx` has `path_dirname`, and this file do |
| [`file_starts_with`](#file-starts-with) | function | — |
| [`file_is_plain_name`](#file-is-plain-name) | function | A name with no `/` and no NUL, and not `.` or `..` — what a single directory entry may be called. |

## Functions
{: #functions}

### `file_read`
{: #file-read}

```burxt
function file_read(path: String) -> String touches files
```

Everything a path holds, as one String.

**This ENDS THE PROGRAM if the file cannot be opened** — `burxt runtime error: cannot open file for reading`, exit 70 — and it exhausts the region if handed a directory. Both are `read_file`'s behaviour and both are measured in the header. It is not a wrong answer, it is no answer at all, and it is why `file_read_maybe` exists.

Use this one only when the file is known to be there — a fixture, a path just written, a file whose absence really should stop everything. Otherwise use `file_read_maybe`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L121)

### `file_read_maybe`
{: #file-read-maybe}

```burxt
function file_read_maybe(path: String) -> Option<String> touches files
```

Everything a path holds, or a stated absence. **This is roadmap §B1's fix.**

`None` means one of three things, and all three are "there is nothing here to read": no such path, no permission to open it, or a directory. `Some("")` means a file that is genuinely empty — the answer `file_read` cannot give, because on a missing file `file_read` gives no answer and ends the program instead.

The existence question is answered by `fopen` rather than by `test -e`, so it is the same open that would do the reading: a path that exists but cannot be opened is not a readable file, and `test -e` would have said it was. There are two opens here, not one — `fopen` to ask, `read_file` to read — so a file deleted between them takes the program down the way `file_read` always does. Closing that window means reading through the handle already open, which is `file_read_bytes`, at the cost of building the answer a byte at a time.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L138)

### `file_read_bytes`
{: #file-read-bytes}

```burxt
function file_read_bytes(path: String) -> Option<[Int]> touches files
```

Every byte of a file, as numbers 0..255, or a stated absence.

**Not `read_file` with `byte_at` over it**, and the difference is not style. `read_file` measures the file with `ftell` before reading it, so a **character device, a FIFO or anything under `/proc` measures as zero and reads as nothing** — the exact trap `tests/pass/os_random_bytes.bx` documents for `/dev/urandom`. This reads until the stream stops, so it works on all of them.

The chain is §A1's: `fread` fills a buffer this function owns, `c_bytes_at` copies the filled part into Burxt, and the pointer is freed here. The buffer is reused across chunks, so a large file costs one 64 KB allocation, not one per read.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L162)

### `file_size`
{: #file-size}

```burxt
function file_size(path: String) -> Option<Int> touches files
```

How many bytes a file holds, or a stated absence.

`None` for a missing path, an unopenable one, or a directory — a directory is ruled out first because `fseek` to its end on Linux answers **9223372036854775807**, which is not a size and would be believed. Measured, not guessed: that is what the probe printed.

For anything that is not a regular file the number is whatever the stream reports, and for an unseekable stream that is nothing useful — `file_read_bytes` is the way to ask how big a device is, by reading it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L200)

### `file_write`
{: #file-write}

```burxt
function file_write(path: String, text: String) -> Int touches files
```

Replaces the file. Answers the number of bytes written.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L221)

### `file_append`
{: #file-append}

```burxt
function file_append(path: String, text: String) -> Int touches files
```

Adds to the end, leaving what was there. Read-modify-write, because the builtin replaces rather than appends — fine for a log line, wrong for a gigabyte, and that is worth knowing before you use it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L228)

### `file_exists`
{: #file-exists}

```burxt
function file_exists(path: String) -> Bool touches commands
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L233)

### `file_is_directory`
{: #file-is-directory}

```burxt
function file_is_directory(path: String) -> Bool touches files
```

Whether the path is a directory.

`opendir` and not `test -d`, and the reason is cost rather than taste: `file_exists` forks a shell, and this question is asked **once per entry** by anything that walks a tree. One syscall against one fork is the difference between a walk that is I/O-bound and one that is not. It also sidesteps `file_quote` entirely, so the single-quote hole below does not apply here.

Symlinks are FOLLOWED, because `opendir` follows them: a link to a directory answers true. That is the right answer for a caller about to read it and the wrong one for a caller avoiding a loop, which is why `file_walk` does not recurse in Burxt at all.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L249)

### `file_is_file`
{: #file-is-file}

```burxt
function file_is_file(path: String) -> Bool touches commands, files
```

Whether the path exists and is not a directory.

**Deliberately not `test -f`, which asks about a REGULAR file.** A device, a socket and a FIFO all answer true here and false to `test -f`, and the question a caller is really asking before `file_read_bytes` is "is there something here that is not a directory". Stated rather than implied, because the two readings differ and only one of them is in the name.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L264)

### `file_delete`
{: #file-delete}

```burxt
function file_delete(path: String) -> Bool touches files
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L268)

### `file_move`
{: #file-move}

```burxt
function file_move(from: String, to: String) -> Bool touches files
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L272)

### `file_copy`
{: #file-copy}

```burxt
function file_copy(from: String, to: String) -> Bool touches commands
```

Copies a file's contents and mode. **Not a directory** — `cp` without `-R` refuses one, and the refusal arrives here as `false`, which is the answer this file wants anyway.

`cp` rather than read-then-write, for two reasons a Burxt loop cannot match: the permission bits come along, and a gigabyte does not pass through the program's memory on the way.

`--` before the paths, because quoting does not stop option parsing: a file honestly named `-r` is a file, not a flag.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L284)

### `file_make_directory`
{: #file-make-directory}

```burxt
function file_make_directory(path: String) -> Bool touches commands
```

0755: readable and executable by everyone, writable by its owner.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L289)

### `file_remove_directory`
{: #file-remove-directory}

```burxt
function file_remove_directory(path: String) -> Bool touches files
```

Removes an **empty** directory.

There is no recursive delete in this file, and that is a decision rather than an omission. `rm -rf` on a path a program computed is the single most expensive mistake a standard library can hand somebody: one empty variable and it is `rm -rf /`. What replaces it is three lines the caller writes in the open, where a reviewer sees the tree being named before it is removed — `file_walk`, then `file_delete` over the answer in reverse, then this. `tests/pass/files_library.bx` ends by doing exactly that, and is the worked example.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L301)

### `file_list_directory`
{: #file-list-directory}

```burxt
function file_list_directory(directory: String) -> Option<[String]> touches commands, files, input
```

Every name in a directory, or `None` if there is no such directory. §B51.

The shell does the reading because `readdir` answers a struct whose field offsets Burxt cannot know — see the header.

**This used to answer `[]` for a directory that is not there**, and the comment that stood here defended it: *"existing published API, and a caller can ask `file_is_directory` first."* That defence was wrong twice over. Empty and absent are different answers and the caller could not tell them apart — §B1's exact shape, which the roadmap calls the silent wrong answer this language exists to refuse, sitting in the standard library where a new reader meets it first. And asking `file_is_directory` first is a race: the directory can go between the two calls.

**The signature changed rather than a second spelling being added**, and the timing is the whole argument: this is the last version before 1.0 states a compatibility promise. Keeping a function that is known to answer wrongly, and adding an honest twin beside it, means carrying the wrong one forever and making every reader ask which of the two they are looking at. There is one caller in this repository. `file_walk` already answers `Option<[String]>`, so this makes the module consistent rather than adding to it.

The status of `ls` is what is CHECKED — not the emptiness of its output, which is the mistake that was here. A missing directory and an empty one produce the same empty text and different exit codes, and only one of those two facts can tell them apart.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L327)

### `file_walk`
{: #file-walk}

```burxt
function file_walk(directory: String) -> Option<[String]> touches commands, files, input
```

Every path under a directory, recursively, sorted — or `None` if there is no such directory.

**One `find`, not a recursion in Burxt**, and the three reasons are worth having in one place:

1. **Cost.** Recursing here means a `ls` fork per directory. `find` walks the whole tree in

```burxt
  one process.
```

2. **Symlink loops.** `find` does not follow symlinks unless asked. A Burxt recursion over

```burxt
  `file_is_directory`, which does follow them, runs forever on `a/b -> a` — and the usual
  patch for that is a depth limit, which is a silently truncated answer.
```

3. **No String is built in a loop.** §D0: the tree's paths come back as lines and are split

```burxt
  once. Nothing here concatenates in a loop, and `len` is never in a loop condition.
```

`sort` is in the pipeline because readdir order is not an order — it differs between filesystems and between runs, and a caller comparing two walks or a test pinning one needs the same answer twice. Sorting also keeps a directory ahead of its own contents, since a parent path is a prefix of every child.

The root itself is not in the answer; an empty directory answers `Some([])`, which is a different fact from `None`.

**A newline inside a filename breaks this**, because the answer travels as lines. So does `file_list_directory`, and so does every `find | while read` in every shell script ever written. Said out loud rather than discovered.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L370)

### `file_temp_directory`
{: #file-temp-directory}

```burxt
function file_temp_directory(prefix: String) -> Option<String> touches files, input
```

A private directory, made atomically, or a stated failure.

The pid is in the name so that two processes do not spend the loop below colliding; the counter is what makes a second call in the SAME process land somewhere else, since the first directory is still there until its owner releases it. Neither is a security claim on its own — `mkdir` is.

`prefix` must be a plain name: a `/` in it would put the directory somewhere the caller did not mean, so it is refused rather than sanitised.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L413)

### `file_temp_path`
{: #file-temp-path}

```burxt
function file_temp_path(prefix: String) -> Option<String> touches files, input
```

A path for a temporary file, inside a fresh private directory. Roadmap §D1m's `temp_file`.

**The file is not created**, and it does not need to be: the directory around it is 0700 and brand new, so nothing can already be sitting on the name and nothing can race to get there. That is the whole reason the directory exists rather than the file — a bare `mktemp`-style file name has to be created to be reserved, and reserving it is the part that is hard to do safely.

Two calls never answer the same path, in the same process or in two. `file_temp_release` is how it goes away, and it takes the directory with it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L441)

### `file_temp_release`
{: #file-temp-release}

```burxt
function file_temp_release(path: String) -> Bool touches files, input
```

Deletes a path from `file_temp_path` **and the private directory holding it**.

Answers whether the directory went away; the file itself may already be gone, which is not a failure. A path this did not hand out is not released — the parent has to look like one of ours, or a caller who passed `/etc/hosts` would have `/etc` attempted.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L451)

### `file_temp_base`
{: #file-temp-base}

```burxt
function file_temp_base() -> String touches input
```

`TMPDIR` if the environment sets one, `/tmp` otherwise — the convention every POSIX tool follows, and the reason a build in a sandbox does not scatter files across a shared `/tmp`.

A relative `TMPDIR` is ignored rather than honoured: it would put scratch files in whatever directory the program happened to be in, which is the one place a caller has other files.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L469)

### `file_quote`
{: #file-quote}

```burxt
function file_quote(path: String) -> String
```

A path made safe for the shell. Single quotes stop every expansion there is, and a single quote inside a path is the one case they cannot hold — so it is refused rather than mishandled.

Quoting does not stop OPTION parsing: `file_quote("-r")` is a correctly quoted `-r`, and the command still reads it as a flag. Where that matters the command gets a `--` — see `file_copy`.

**The refusal goes to `print_error` and not `print`, and it used to go to `print`.** A library writing a diagnostic to stdout corrupts the output of every program that is piped: a caller doing `file_read` of what a Burxt program printed would find this sentence in the middle of their data. `lib/log.bx` makes the same argument at length and calls it *"not a preference"* — this was the one place in the library still disagreeing with it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L497)

### `file_split_lines`
{: #file-split-lines}

```burxt
function file_split_lines(text: String) -> [String]
```

Split on newlines, dropping the empty piece a trailing newline leaves. Local rather than borrowed from lib/string.bx, so `use "lib/files.bx"` pulls in files and nothing else.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L515)

### `file_last_slash`
{: #file-last-slash}

```burxt
function file_last_slash(path: String) -> Int
```

The last `/`, or -1. Local for the same reason `file_split_lines` is: `lib/path.bx` has `path_dirname`, and this file does not depend on it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L534)

### `file_starts_with`
{: #file-starts-with}

```burxt
function file_starts_with(text: String, prefix: String) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L547)

### `file_is_plain_name`
{: #file-is-plain-name}

```burxt
function file_is_plain_name(name: String) -> Bool
```

A name with no `/` and no NUL, and not `.` or `..` — what a single directory entry may be called.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/files.bx#L556)


{% endraw %}

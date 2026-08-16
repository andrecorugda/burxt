---
layout: doc
title: lib/path.bx
section: reference
description: "POSIX paths, taken apart and put back together, lexically."
---


# `lib/path.bx`

POSIX paths, taken apart and put back together, lexically.

```burxt
use "lib/path.bx";
```

Seven functions: `path_join`, `path_basename`, `path_dirname`, `path_extension`, `path_stem`, `path_is_absolute`, `path_normalise`. Before this file a Burxt program that wanted the directory a file was in scanned backwards for a byte 47 itself, and every program that did so got a slightly different answer for `"/"`.

**Nothing here touches the filesystem.** Every function is a String going in and a String coming out, so nothing can fail, nothing needs an effect, and every one of them is `pure`. That is a deliberate boundary, not an accident of what was easy: see the note on `path_normalise` for the one place where a caller could mistake a lexical answer for a real one.

---- POSIX only, and that is a 1.0 decision -------------------------------------------

The separator is `/`, and only `/`. A backslash is an ordinary byte in a filename, `C:\x` is a relative path whose single component is `C:\x`, and a drive letter means nothing. Windows paths are `spec/ROADMAP-2.0.md`'s problem and the reason they are not attempted here is that a half-portable path library is worse than a POSIX one: it answers plausibly on Windows input while getting UNC prefixes, drive-relative paths (`C:x` is not `C:\x`) and case folding wrong, and a caller cannot tell which of those it just hit. This file's answers are wrong on Windows in a way that is *stated*, which is the only kind of wrong a library gets to be.

**`//` is not a special root.** POSIX permits an implementation to treat exactly two leading slashes as a distinct root, and `python3 -c 'import posixpath; posixpath.normpath("//a")'` answers `//a` for that reason. Linux does not implement it, so `path_normalise("//a")` answers `/a` here. Measured against coreutils before choosing: `dirname //a` on this machine answers `/` too, so the collapse agrees with the tools a caller already has.

---- where D0's chunk rule applies, and where it does not ------------------------------

`spec/1.0/ROADMAP-1.0.md` §D0 requires a chunk list joined PAIRWISE for anything that builds a String, because `out = out + piece` in a loop cost this compiler 1,132 MB against 169 MB for the identical output. **Exactly one function here builds a String in a loop** — `path_normalise`, which reassembles the components it kept — and it uses the chunk list, in `path_merge` below.

The other six do not loop over anything they are building. `path_join` is two `substring`s and at most two `+`; `path_basename`, `path_dirname`, `path_extension` and `path_stem` are one `substring` each; `path_is_absolute` allocates nothing at all. Chunking a two-piece concatenation would be more machinery than the thing it protects, and this is said here rather than left for a reader to verify seven times.

The honest measurement about `path_merge` in particular: a POSIX path is capped at `PATH_MAX` (4096 bytes on Linux), so the left fold it replaces was quadratic with a *ceiling* — about 4 MB of copying for the worst input anyone can hand it, not the 963 MB the compiler paid. The pairwise merge is used anyway, because it is nine lines and because a library is the wrong place to teach the shape this project has now paid for four times. It is not used because it was measured to matter here; it was not.

**`len(s)` walks the String**, so no loop below leaves it in the condition — each one reads the length once into a local first. That alone made the lexer quadratic once (§D0).

---- the decisions, and where each one is written down ---------------------------------

Every function's comment states the case that decided it. The four worth naming up here, because they are the four a caller is most likely to assume the other way:

1. **`path_join("a", "/b")` is `"a/b"`, not `"/b"`.** Python and Rust both let an absolute

```burxt
  right-hand side DISCARD the left. See `path_join`.
```

2. **`path_extension("archive.tar.gz")` is `"gz"`**, one extension, without the dot. 3. **`path_extension(".hidden")` is `""`** — a leading dot hides a file, it does not extend it. 4. **`path_normalise("../..")` is `"../.."`.** A relative path KEEPS its leading `..`. This is

```burxt
  the one that makes a path library dangerous rather than merely wrong.
```

And one invariant that holds for every input, tested exhaustively in `tests/pass/path_library.bx`:

```burxt
 path_stem(p) + "." + path_extension(p) == path_basename(p)    when the extension is non-empty
 path_stem(p)                           == path_basename(p)    when it is empty
```

That is what makes the `.hidden`, `..`, `a.` and `archive.tar.gz` answers a single rule rather than four special cases: the stem and the extension always partition the basename.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`path_last_separator`](#path-last-separator) | function | Where the last separator sits, or -1. Backwards, because every question this file asks is about the END of a path. |
| [`path_without_trailing`](#path-without-trailing) | function | `text` with every trailing separator removed. `"a/b/"` becomes `"a/b"`, `"///"` becomes `""`, and `""` stays `""`. |
| [`path_is_absolute`](#path-is-absolute) | function | Does this path start from the root? |
| [`path_basename`](#path-basename) | function | The last component. Measured against `basename(1)` on this machine, and it agrees on all of it: |
| [`path_dirname`](#path-dirname) | function | Everything before the last component. Also measured against `dirname(1)`, which is where the three answers a reader woul |
| [`path_extension_dot`](#path-extension-dot) | function | Where the extension's dot sits in a BASENAME, or -1 when there is no extension. |
| [`path_extension`](#path-extension) | function | The extension, **without the dot**, or `""`. |
| [`path_stem`](#path-stem) | function | The basename without its extension. |
| [`path_join`](#path-join) | function | Two pieces with **exactly one separator between them**, however many each side brought. |
| [`path_merge`](#path-merge) | function | The chunk list, joined PAIRWISE — §D0's shape, and `join_chunks` in `src/burxt-compiler/emit.bx` is the reference this f |
| [`path_normalise`](#path-normalise) | function | **Purely lexical. It does not resolve symlinks, and `a/b/..` is therefore not always `a`.** If `b` is a symlink to `/els |

## Functions
{: #functions}

### `path_last_separator`
{: #path-last-separator}

```burxt
pure function path_last_separator(text: String) -> Int
```

Where the last separator sits, or -1. Backwards, because every question this file asks is about the END of a path.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/path.bx#L88)

### `path_without_trailing`
{: #path-without-trailing}

```burxt
pure function path_without_trailing(text: String) -> String allocates
```

`text` with every trailing separator removed. `"a/b/"` becomes `"a/b"`, `"///"` becomes `""`, and `""` stays `""`.

The empty answer is load-bearing rather than a degenerate case: a path that is nothing but separators is the ROOT, and the two callers below both check for it. Collapsing that check into this function — answering `"/"` — would be wrong for `path_join`, which needs to know that the left side contributed no component.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/path.bx#L107)

### `path_is_absolute`
{: #path-is-absolute}

```burxt
pure function path_is_absolute(path: String) -> Bool
```

Does this path start from the root?

One byte, and no filesystem: `path_is_absolute("/nowhere")` is true whether or not that file exists. The empty path is not absolute.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/path.bx#L121)

### `path_basename`
{: #path-basename}

```burxt
pure function path_basename(path: String) -> String allocates
```

The last component. Measured against `basename(1)` on this machine, and it agrees on all of it:

```burxt
 path_basename("/a/b/")  == "b"      trailing separators belong to the framing, not the name
 path_basename("/")      == "/"      the root's name is the root
 path_basename("")       == ""       nothing in, nothing out
 path_basename("a")      == "a"
 path_basename("//")     == "/"
```

The trailing-slash case is the one that decides this function. `"/a/b/"` names the directory `b`, and a caller asking for its basename wants `"b"` — a library answering `""` because the last byte is a separator has turned a shell habit into an empty string, and the caller finds out three functions later.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/path.bx#L137)

### `path_dirname`
{: #path-dirname}

```burxt
pure function path_dirname(path: String) -> String allocates
```

Everything before the last component. Also measured against `dirname(1)`, which is where the three answers a reader would not guess come from:

```burxt
 path_dirname("/")       == "/"      the root's parent is the root, and the kernel agrees
 path_dirname("a")       == "."      no separator means "the directory I am in"
 path_dirname("")        == "."      the same, and NOT "" — see below
 path_dirname("/a/b/")   == "/a"
 path_dirname("/a")      == "/"
 path_dirname("//a//b//") == "//a"
```

**`""` for `path_dirname("")` would be the dangerous answer**, because `""` is not a path any `open` accepts, so it converts a harmless empty input into a failure at the syscall — three stack frames from the mistake. `"."` is a real directory and it is the right one: the components of `""` are no components, and the directory holding no components is where you are.

Interior separators are left exactly as they were: `path_dirname("a//b//c")` is `"a//b"`, not `"a/b"`, which is what `dirname(1)` answers too. This function is a lexical prefix, and normalising is `path_normalise`'s job — a function that quietly did both would make it impossible to do only one.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/path.bx#L170)

### `path_extension_dot`
{: #path-extension-dot}

```burxt
pure function path_extension_dot(name: String) -> Int
```

Where the extension's dot sits in a BASENAME, or -1 when there is no extension.

Two conditions, and each was chosen by a case rather than by symmetry:

* **A dot at index 0 does not count.** `".hidden"` is a hidden file on POSIX and `"hidden"` is

```burxt
 its whole name — the dot is part of the name, not a separator before an extension. A library
 answering `"hidden"` here makes `path_stem(".hidden")` answer `""`, and a caller that builds
 a sibling filename from the stem has just created a file called `.txt`.
```

* **A dot at the very end does not count.** `"a."` and `".."` both hit this. `".."` matters:

```burxt
 it is the parent directory, and reading its extension as empty-after-the-second-dot would
 leave `path_stem("..")` as `"."`, which names a DIFFERENT directory. That is the shape of
 bug this whole file exists to not have.
```

The scan stops at the first dot found from the right, so `"a.b."` answers -1 rather than looking past the trailing dot to find `.b`. `"b"` is not the extension of `"a.b."` — nothing is.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/path.bx#L209)

### `path_extension`
{: #path-extension}

```burxt
pure function path_extension(path: String) -> String allocates
```

The extension, **without the dot**, or `""`.

```burxt
 path_extension("archive.tar.gz") == "gz"
 path_extension(".hidden")        == ""
 path_extension("a/b.c/d")        == ""
 path_extension("..")             == ""
 path_extension("a.")             == ""
```

**`"gz"` and not `"tar.gz"`, and this is a decision rather than laziness.** A compound extension is not derivable — `.tar.gz` is a real pair and `.tar.zst` is too, but `.min.js` is not `js` applied to `min`, and no rule distinguishes them. The only mechanism that gets compound extensions right is a curated list, and a curated list is wrong the day someone invents a suffix: a library that knows `tar.gz` and not `tar.br` is *worse* than one that always answers the last component, because a caller cannot predict which of the two behaviours they will get. So: the extension is always the last one, and a caller who wants the compound asks twice — `path_extension(path_stem(path))` answers `"tar"`.

**Without the dot**, because `path_extension(p) == "gz"` is the comparison every caller writes, and a leading dot makes that read `== ".gz"` — which is exactly the kind of thing that is right in four places and forgotten in the fifth. Python's `splitext` keeps the dot; Rust's `Path::extension` does not; this follows Rust, and the stem/extension invariant in the header is stated so a caller can reassemble a filename without guessing.

**An empty extension and no extension are the same answer.** `"a."` and `"a"` both answer `""`. If a program needs to tell those apart it is asking about bytes rather than about paths, and `byte_at` is right there.

The `"a/b.c/d"` case is the one a naive implementation gets wrong: the dot is in a DIRECTORY name, and `d` has no extension. Working from the basename rather than from the whole path is the only reason that comes out right.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/path.bx#L257)

### `path_stem`
{: #path-stem}

```burxt
pure function path_stem(path: String) -> String allocates
```

The basename without its extension.

```burxt
 path_stem("archive.tar.gz") == "archive.tar"
 path_stem(".hidden")        == ".hidden"
 path_stem("/a/b/")          == "b"
 path_stem("..")             == ".."
 path_stem("a.")             == "a."
```

The same dot as `path_extension` finds, so the two always partition the basename — see the invariant in the header. `path_stem(".hidden")` being the whole name is the payoff for the index-0 rule: the alternative answers `""`, and a caller writing `path_stem(p) + ".bak"` would produce `.bak` instead of `.hidden.bak`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/path.bx#L278)

### `path_join`
{: #path-join}

```burxt
pure function path_join(left: String, right: String) -> String allocates
```

Two pieces with **exactly one separator between them**, however many each side brought.

```burxt
 path_join("a/", "/b")  == "a/b"      the case this function exists for
 path_join("a", "")     == "a"        an empty piece adds nothing, not a trailing slash
 path_join("", "b")     == "b"
 path_join("/", "a")    == "/a"       the root survives being stripped
 path_join("a", "/b")   == "a/b"      and NOT "/b" — see below
```

**An absolute right-hand side does not discard the left, and that is the one place this deliberately disagrees with every other path library.** `os.path.join("a", "/b")` in Python and `Path::new("a").join("/b")` in Rust both answer `/b`: the second argument, alone. That rule is convenient when both arguments are literals in the same file and a trap in the case that actually occurs — a trusted base directory joined to a name that came from outside the program. `path_join(uploads, name)` with `name` of `"/etc/passwd"` answers `/etc/passwd` under Python's rule, having silently thrown away the sandbox it was given. Burxt exists to not have that shape of bug (`spec/DESIGN.md`: strict enough that an agent cannot err), so the surprising behaviour is the one this does not have.

A caller who WANTS the reset writes it, in one line a reviewer can see:

```burxt
 let full: String = if path_is_absolute(name) { name } else { path_join(base, name) };
```

**This is not a containment check.** `path_join(uploads, "../../etc/passwd")` answers `uploads/../../etc/passwd`, which escapes — `..` is `path_normalise`'s business, and confining a path to a directory means normalising first and then checking the prefix. Said here because "join does not let you out" is exactly the guarantee a reader might infer from the paragraph above, and it is not one this function makes.

No normalising otherwise: `path_join("a//b", "c")` is `"a//b/c"`. Only the joint is touched.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/path.bx#L318)

### `path_merge`
{: #path-merge}

```burxt
pure function path_merge(chunks: [String]) -> String allocates
```

The chunk list, joined PAIRWISE — §D0's shape, and `join_chunks` in `src/burxt-compiler/emit.bx` is the reference this follows.

Repeated pairwise merge rather than a left fold, because a left fold rebuilds the whole prefix at every step: that is the same quadratic §D0 exists to escape, one level up. See the header for the honest note about how much this wins on a path specifically (a ceiling of a few megabytes rather than the compiler's 963 MB) and why it is used regardless.

Not a call to `string_join` in `lib/string.bx`: that one is a left fold today. When it becomes pairwise this can delegate and go away.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/path.bx#L357)

### `path_normalise`
{: #path-normalise}

```burxt
pure function path_normalise(path: String) -> String allocates
```

**Purely lexical. It does not resolve symlinks, and `a/b/..` is therefore not always `a`.** If `b` is a symlink to `/elsewhere/c`, then `a/b/..` is `/elsewhere` and this function answers `a`. That is why the name is `normalise` and not `canonicalise` or `realpath`: the honest version needs `realpath(3)`, which returns a `char*` the caller must free, and Burxt's C boundary does not describe that ownership yet (`lib/README.md`, "Safety at the boundary"). A program that must know where a path really leads has to ask the filesystem, and cannot ask this.

It also does not touch the filesystem in the ordinary sense — no component is checked for existence, and `path_normalise` of a path to a file that was never created answers just fine.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/path.bx#L422)


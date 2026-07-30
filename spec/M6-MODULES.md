# Burxt — Modules (M6)

> Status: **DONE (v0.0.81–v0.0.82).** Both compilers resolve `use`, and the acceptance test
> that matters passed: `examples/stage1.bx` is now 105 lines that `use` five modules, and the
> fixpoint still holds — the compiler compiles its split self byte-identically.
>
> Original status: **specified, implementing.** The blocker for everyone who is not the author:
> `examples/stage1.bx` is 4,996 lines in one file because it has no choice, and nothing
> multi-file, multi-author or reusable is possible until this exists.

## 0. What has to become possible

```text
// lexer.bx
class Tok { kind: Int, start: Int, length: Int }
function scan(src: String) -> Int { ... }

// main.bx
use "lexer.bx";

region r {
    print(scan(read_file(argument(1))));
}
```

Two files, one program, and the compiler reads both.

## 1. Decisions

### Decision 1 — a file is a module, and `use` names a path

```text
use "lexer.bx";
use "front/parser.bx";
```

The path is **relative to the file that writes it**, so a directory of Burxt files can be
moved without editing anything inside it. No search path, no include directories, no
environment variable: a path that means one file on one machine means the same file
everywhere. When packaging arrives it will add a way to name a *dependency*, and that is a
different question from naming a file.

**Why a path rather than a name.** A name (`use lexer;`) needs a resolution rule — which
directory, which extension, which precedence — and every language that has one has a page of
documentation about it. A path is already unambiguous, and the reader knows what it means
without learning anything.

### Decision 2 — everything a module declares is visible, and there is no `pub` yet

A `use`d file's functions, records, enums, interfaces and impls are all available to the file
that used it. No visibility annotations in this slice.

**Why.** `pub` doubles the annotation burden on every declaration, and its value is hiding —
which matters when strangers depend on your module and not before. Burxt has no package
ecosystem yet, so the cost is certain and the benefit is speculative. Deferred with a
trigger, not forgotten: **when a library needs to hide an internal helper from its
consumers, `pub` earns its place.** Name collisions are already an error, which is the
half of the problem that bites today.

### Decision 3 — a module holds declarations, not statements

```text
// refused
use "helpers.bx";      // where helpers.bx contains: print("loaded!");
```

```text
error: a module holds declarations, not statements: `print` would run when the file was
       used, and a `use` is not a call
```

Top-level statements are what makes a file a *program*. A file that runs when it is imported
is the import side-effect problem, and every language that allows it grows a convention
against it. Refused rather than discouraged.

The file being **compiled** may of course have statements — that is what makes it the
program. The rule is about files reached through `use`.

### Decision 4 — each file is compiled once, and cycles are fine

`a` uses `b`, `b` uses `a`: allowed, and it works. Burxt collects every declaration before
checking any body, so mutual reference across files needs no forward declarations, no header
files, and no ordering rule. A file already seen is not read again.

This is a real advantage over `#include`, and it falls out of the two-pass checker rather
than being designed for.

### Decision 5 — one buffer, one source map

The compiler concatenates the sources it read, in the order it read them, and works on that
single buffer. A `Span` stays what it is: **a byte range**. The *renderer* consults a source
map — a list of (path, start, length) — to say which file an offset fell in.

**Why not a file index in every Span.** It would touch every diagnostic, both compilers, and
every place a span is built or compared, in exchange for information only the renderer needs.
The map is ten lines and it is consulted once per error message. Stage-1 gets the same design
for the same reason, which also keeps the two implementations comparable.

The cost, stated: the buffer holds every file, so a compile's memory is the sum of its
sources. For a language whose compiler is 5,000 lines that is not a constraint, and if it
ever becomes one the answer is a per-file arena rather than a span redesign.

## 2. What this must NOT do

- **NO implicit prelude.** Nothing is in scope that a file did not ask for. A program that
  reads `scan(...)` must contain the `use` that brought `scan` in.
- **NO glob imports** (`use "front/*.bx"`). A reader who cannot tell where a name came from
  cannot read the program, and a build that changes when a file appears is not reproducible.
- **NO renaming or aliasing yet** (`use "x.bx" as y`). It only matters once collisions are
  common, which is once `pub` and packaging exist.
- **NO conditional compilation.** A file that means different things on different machines
  is the thing this language exists to refuse.
- **NO running code at import.** See Decision 3.
- **NO circular-import error.** See Decision 4 — it works, so refusing it would be a rule
  with nothing behind it.

## 3. Deferred, with triggers

| Feature | Why deferred | Earns its place when |
|---|---|---|
| `pub` / visibility | Cost is certain, benefit is speculative before packaging | A library must hide a helper from its consumers |
| Aliasing (`as`) | Only matters with frequent collisions | Two used modules declare the same name and both are needed |
| Named dependencies (`use burxt/json`) | That is packaging, not modules | A dependency lives outside the source tree |
| A per-file arena | The one-buffer cost is not felt yet | A compile's memory is measurably a problem |
| Re-export | Needs `pub` first | A module wants to present another's type as its own |

## 4. Acceptance

1. Two files, one program: `main.bx` uses `lexer.bx`, calls a function declared there, and
   runs.
2. A record declared in one file is used in another, with the same layout either way.
3. A diagnostic in a used file names **that file** and the right line, not an offset into a
   concatenated buffer.
4. A file used twice — directly and through another module — is compiled once.
5. Two files that use each other compile, and each may call the other's functions.
6. A used file containing a top-level statement is refused with Decision 3's message.
7. A missing path is refused, naming the file that asked for it.
8. **`examples/stage1.bx` splits into `lexer.bx`, `parser.bx`, `check.bx`, `emit.bx` and
   `main.bx`, and the fixpoint still holds.** This is the real acceptance test: if the
   compiler cannot be split, modules are not done.
9. Both compilers implement it, and the differential test still passes — which means
   stage-1 must parse and check a `use` too, not merely tolerate it.

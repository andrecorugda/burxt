---
title: Modules
---

# 8. Modules

## The problem, as it actually arrives

You add a module. It does not resolve. So you go and read a page of documentation about a search
path, and a manifest, and whether `lexer` means `./lexer.bx` or `./lexer/mod.bx` or something on an
environment variable — and then it works on your machine and not in CI.

Or the resolution works fine and the *ordering* does not: file A needs a type from file B, B needs a
function from A, and you are writing forward declarations, or a header, or splitting a file for a
reason that has nothing to do with the program.

Burxt has one rule instead, and it is smaller than it sounds.

## `use` is a paste, not an import

```burxt
use "lexer.bx";
```

That line is handled **before anything is lexed**. The compiler reads `lexer.bx` and puts its text
into the buffer. There is no module object, no namespace, no symbol table lookup — by the time
anything is checked there are no files at all, only one long program.

<svg viewBox="0 0 640 268" role="img" aria-label="use concatenates files into one buffer before anything is checked" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .b { fill: #fff; stroke: #111; stroke-width: 1.5; }
    .t { font: 12px ui-monospace, monospace; fill: #111; }
    .g { font: 11px ui-monospace, monospace; fill: #888; }
    .s { font: 11px ui-monospace, monospace; fill: #b00; }
    .a { stroke: #111; stroke-width: 1.5; fill: none; marker-end: url(#a8); }
    .sep { stroke: #888; stroke-width: 1; stroke-dasharray: 4 3; }
    @media (prefers-color-scheme: dark) {
      .b { fill: #1b1b1b; stroke: #ddd; } .t { fill: #eee; } .s { fill: #ff8080; }
      .a { stroke: #ddd; } .g, .sep { stroke: #999; fill: #999; }
    }
  </style>
  <defs>
    <marker id="a8" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/>
    </marker>
  </defs>

  <rect class="b" x="20" y="30" width="180" height="56" rx="4"/>
  <text class="t" x="32" y="52">lexer.bx</text>
  <text class="g" x="32" y="70">declarations only</text>

  <rect class="b" x="20" y="104" width="180" height="56" rx="4"/>
  <text class="t" x="32" y="126">lib/option.bx</text>
  <text class="g" x="32" y="144">declarations only</text>

  <rect class="b" x="20" y="178" width="180" height="56" rx="4"/>
  <text class="t" x="32" y="200">main.bx</text>
  <text class="g" x="32" y="218">…and statements</text>

  <path class="a" d="M200 58 L426 58"/>
  <path class="a" d="M200 132 L426 132"/>
  <path class="a" d="M200 206 L426 206"/>

  <rect class="b" x="430" y="24" width="190" height="216" rx="4"/>
  <line class="sep" x1="430" y1="96" x2="620" y2="96"/>
  <line class="sep" x1="430" y1="168" x2="620" y2="168"/>
  <text class="g" x="442" y="46">lexer.bx</text>
  <text class="g" x="442" y="118">lib/option.bx</text>
  <text class="g" x="442" y="190">main.bx</text>
  <text class="s" x="442" y="230">one program</text>

  <text class="g" x="20" y="262">plus a map back to every original line, so an error still says lexer.bx:3</text>
</svg>

Everything else on this page follows from that one picture.

```burxt
// lexer.bx — declarations only
class Tok { kind: Int, start: Int }
function scan(text: String) -> Int { return len(text); }
```

```burxt
// main.bx
use "lexer.bx";

let t: Tok = Tok { kind: 7, start: 0 };
print(scan("hello"));
```

```sh
$ burxt run main.bx
5
```

## The path is relative to the file that writes it

So a directory of Burxt files moves without editing anything inside it. There is no search path, no
include directory and no environment variable: **a path that means one file on one machine means the
same file everywhere.**

That is why it is a path and not a name. `use lexer;` needs a resolution rule — which directory,
which extension, which precedence — and every language that has one also has the page of
documentation from the top of this page. A path is already unambiguous.

## Imports come first

`use` lines go at the top of a file, before any other item. Blank lines and comments may sit among
them; anything else ends the header. Look at the buffer in the diagram and you can see why: pasted
text has to land somewhere, and "above everything" is the only position that needs no rule.

## A module holds declarations, not statements

```burxt
// helpers.bx
print("loaded!");        // ← refused
```

```
error: a module holds declarations, not statements: this would run when `helpers.bx` was
       used, and a `use` is not a call
```

Top-level statements are what make a file a *program*. A file that runs when you import it is the
import-side-effect problem, and every language that allows it grows a convention against it. The file
you are **compiling** may of course have statements — that is exactly what makes it the program.

## Cycles are fine, and need no headers

```burxt
// a.bx
use "b.bx";
function from_a() -> Int { return from_b() + 1; }

// b.bx
use "a.bx";
function from_b() -> Int { return 41; }
```

This compiles and `from_a()` is `42`. Each file is read once; a file already in the buffer is not
pasted again. Burxt then collects **every** declaration before checking **any** body, so mutual
reference across files needs no forward declarations, no header files and no ordering rule.

That is a real advantage over `#include`, and it fell out of having a two-pass checker rather than
being designed for.

## Errors name your file, not the buffer

The compiler keeps a map from every byte of the buffer back to the file it came from, so a mistake
inside a used module reports *that module* and *its own* line number:

```
error: cannot + Decimal<2> and Decimal<4>: scales must match.
 --> bad.bx:3:12
```

Without that map every error in the program would point at one enormous file, which is the failure
mode this design would otherwise have.

## What is visible, and where privacy actually lives

Everything a module declares — functions, classes, enums, interfaces, implementations — is available
to the file that used it. There is no `pub`.

That is not the oversight it looks like, and the diagram is the reason: **there are no files by the
time anything is checked**, so there is nothing for a file-level `pub` to be relative to. Adding one
would mean teaching the whole checker about a boundary it currently does not need to know exists.

Privacy is real here, it just lives one level down: **the class is the boundary.** `private` on a
field or a method means *this class's own methods, and nothing else* — including top-level code in
the very same file. See [the sealed box](03-types.md#piece-two-private).

Name collisions across modules are already an error, which is the half of the problem that bites
today. The deferral, and the trigger for revisiting it, are in
[`spec/M6-MODULES.md`](../../spec/M6-MODULES.md).

## Next

[Generics](09-generics.md) — one definition, one copy per type that uses it. Which is also what
makes `lib/option.bx` and `lib/result.bx` libraries rather than keywords.

---
title: Modules
description: A use is a photocopy, not an import — the text lands in your buffer before anything is checked, so there is nothing to resolve and cycles cost nothing.
---

# 8. Modules

## What this is for
{: #what-this-is-for}

You add a module. It does not resolve. So you go and read a page of documentation about a search
path, and a manifest, and whether `lexer` means `./lexer.bx` or `./lexer/mod.bx` or something on an
environment variable — and then it works on your machine and not in CI.

Or the resolution works fine and the *ordering* does not: file A needs a type from file B, B needs a
function from A, and you are writing forward declarations, or a header, or splitting a file for a
reason that has nothing to do with the program.

Burxt has one rule instead, and it is smaller than it sounds.

## Think of a photocopier
{: #think-of-a-photocopier}

You are working from a notebook. You need a page from another notebook, so you photocopy it and tape it
in. Now it is in your notebook. Not referenced, not linked, not looked up — *there*.

Nobody has to agree on a filing system, because nothing is being filed. And it does not matter that the
other notebook also has a copy of a page from yours: two photocopies do not make a loop, they make two
pages.

<figure>
<svg viewBox="0 0 680 264" role="img" aria-label="A use is a photocopy: two module files are copied into one buffer before anything is lexed, so there are no files left by the time the program is checked and a cycle is just two copies" style="max-width:100%;height:auto;">
  <style>
    .sheet { fill: #ffffff; stroke: #1d1d1f; stroke-width: 1.6; }
    .buf   { fill: #ffffff; stroke: #1d1d1f; stroke-width: 2; }
    .band  { fill: #0071e3; opacity: .07; }
    .rule  { fill: none; stroke: #d2d2d7; stroke-width: 1; }
    .sep   { stroke: #3a3a3e; stroke-width: 1; stroke-dasharray: 4 3; }
    .copy  { fill: none; stroke: #0071e3; stroke-width: 2; marker-end: url(#mc); }
    .h     { font: 600 13.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .t     { font: 11.5px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #1d1d1f; }
    .blue  { font: 600 11.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #0071e3; }
    .cap   { font: 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
  </style>
  <defs>
    <marker id="mc" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="#0071e3"/>
    </marker>
  </defs>

  <text class="h" x="8" y="18">Three files</text>

  <rect class="sheet" x="14" y="32" width="130" height="54" rx="5"/>
  <text class="t" x="24" y="52">items.bx</text>
  <line class="rule" x1="24" y1="62" x2="134" y2="62"/>
  <line class="rule" x1="24" y1="74" x2="112" y2="74"/>

  <rect class="sheet" x="14" y="100" width="130" height="54" rx="5"/>
  <text class="t" x="24" y="120">tax.bx</text>
  <line class="rule" x1="24" y1="130" x2="134" y2="130"/>
  <line class="rule" x1="24" y1="142" x2="104" y2="142"/>

  <rect class="sheet" x="14" y="168" width="130" height="54" rx="5"/>
  <text class="t" x="24" y="188">till.bx</text>
  <line class="rule" x1="24" y1="198" x2="134" y2="198"/>
  <line class="rule" x1="24" y1="210" x2="118" y2="210"/>

  <path class="copy" d="M154 59 q56 0 76 24"/>
  <path class="copy" d="M154 127 h76"/>
  <path class="copy" d="M154 195 q56 0 76 -24"/>
  <text class="blue" x="164" y="88">photocopied</text>

  <text class="h" x="272" y="18">One buffer, before anything is lexed</text>

  <rect class="buf"  x="278" y="32" width="240" height="190" rx="8"/>
  <rect class="band" x="286" y="40" width="224" height="58" rx="4"/>
  <text class="t" x="296" y="60">items.bx</text>
  <line class="rule" x1="296" y1="72" x2="494" y2="72"/>
  <line class="rule" x1="296" y1="86" x2="452" y2="86"/>
  <line class="sep"  x1="286" y1="104" x2="510" y2="104"/>
  <rect class="band" x="286" y="110" width="224" height="50" rx="4"/>
  <text class="t" x="296" y="130">tax.bx</text>
  <line class="rule" x1="296" y1="144" x2="470" y2="144"/>
  <line class="sep"  x1="286" y1="166" x2="510" y2="166"/>
  <rect class="band" x="286" y="172" width="224" height="42" rx="4"/>
  <text class="t" x="296" y="192">till.bx</text>
  <line class="rule" x1="296" y1="204" x2="486" y2="204"/>

  <text class="cap" x="548" y="60">No module</text>
  <text class="cap" x="548" y="78">object. No</text>
  <text class="cap" x="548" y="96">namespace. No</text>
  <text class="cap" x="548" y="114">search path.</text>
  <text class="cap" x="548" y="146">By the time</text>
  <text class="cap" x="548" y="164">anything is</text>
  <text class="cap" x="548" y="182">checked there</text>
  <text class="cap" x="548" y="200">are no files.</text>

  <text class="cap" x="8" y="252">Which is why a cycle needs no header, and why an error still names the file you wrote it in.</text>
</svg>
<figcaption>Two files that <code>use</code> each other are just two photocopies. There is no ordering
problem to solve, because there is no ordering.</figcaption>
</figure>

## A step closer
{: #a-step-closer}

```burxt
use "lexer.bx";
```

That line is handled **before anything is lexed**. The compiler reads `lexer.bx` and puts its text into
the buffer. There is no module object, no namespace, no symbol table lookup — by the time anything is
checked there are no files at all, only one long program.

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

## In code
{: #in-code}

### The path is relative to the file that writes it

So a directory of Burxt files moves without editing anything inside it. There is no search path, no
include directory and no environment variable: **a path that means one file on one machine means the
same file everywhere.**

That is why it is a path and not a name. `use lexer;` needs a resolution rule — which directory,
which extension, which precedence — and every language that has one also has the page of
documentation from the top of this page. A path is already unambiguous.

### Imports come first

`use` lines go at the top of a file, before any other item. Blank lines and comments may sit among
them; anything else ends the header. Look at the buffer in the diagram and you can see why: pasted
text has to land somewhere, and "above everything" is the only position that needs no rule.

### A module holds declarations, not statements

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

### Cycles are fine, and need no headers

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

### Errors name your file, not the buffer

The compiler keeps a map from every byte of the buffer back to the file it came from, so a mistake
inside a used module reports *that module* and *its own* line number:

```
error: cannot + Decimal<2> and Decimal<4>: scales must match.
 --> bad.bx:3:12
```

Without that map every error in the program would point at one enormous file, which is the failure
mode this design would otherwise have.

## Why it is built this way
{: #why-it-is-built-this-way}

**Because the alternative is a page of documentation and a CI failure.** A search path, a manifest, and a
convention about whether `lexer` means `./lexer.bx` or `./lexer/mod.bx` are three things to get right
before any code runs, and they fail differently on different machines. A relative path to a file that
exists fails the same way everywhere: it says which path, from which file.

**Cycles stop being a problem rather than being solved.** No forward declarations, no headers, no
splitting a file for a reason that has nothing to do with the program. The checker sees one buffer, and
declaration order in one buffer does not matter.

**It is the smallest thing that could work**, and that is a deliberate position rather than an
apology. There is no `pub`, no re-export, no aliasing and no versioning — and none of that has been
needed by the compiler that is written in this language and imports its own eight files.

### What is visible, and where privacy actually lives

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
[`spec/M6-MODULES.md`](https://github.com/andrecorugda/burxt/blob/main/spec/M6-MODULES.md).

## What it costs
{: #what-it-costs}

**There is no `pub`, so a module exports everything it declares.** A helper you meant to keep to
yourself is visible to whoever used the file.

**A name collision across modules is an error**, which is the half of the missing-`pub` problem that
bites today — two modules cannot both declare `parse`.

**Nothing is cached or compiled separately.** Every build reads every file. That is fine at the size the
compiler itself is, and it is not a module system that will scale to a thousand files unchanged.

**A `use` cannot be conditional.** No feature flags, no platform-specific module. The imports are the
first lines of the file and that is all they are.

**And the deferral is recorded rather than hidden**, along with the trigger for revisiting it:
[`spec/M6-MODULES.md`](https://github.com/andrecorugda/burxt/blob/main/spec/M6-MODULES.md).

## When you reach for it
{: #when-you-reach-for-it}

<div class="tablewrap" markdown="1">

| You want | Do this |
|---|---|
| to split a program up | one file per group of declarations, and `use "that.bx";` at the top |
| the standard library | `use "lib/string.bx";` — the path is relative to the file writing it |
| two files that need each other | write both `use` lines. Cycles are fine and need no headers |
| something to be private | put it in a `class` and mark it `private`. The class is the boundary, not the file |
| to run code at startup | put it in the file you compile, not in a module. A module holds declarations |

</div>

The rule of thumb: **a module is a group of declarations with a filename.** If you find yourself wanting
it to be more than that, the thing you want is probably a class.

## Examples
{: #examples}

**Two files, one program.** `shapes.bx` holds declarations; `till.bx` is what you compile:

```burxt
// shapes.bx — one module, holding declarations only.
class Item { sku: String, price: Decimal<2> }

function shown(item: Item) -> String {
    return item.sku + " at " + to_string(item.price);
}
```

```burxt
use "shapes.bx";

let rice: Item = Item { sku: "RICE", price: $52.75 };
print(shown(rice));
```

```
RICE at 52.75
```

No manifest, no build file, and nothing to configure — the path is relative to the file that wrote it.

**And an error still names the file you wrote it in**, which is the thing a concatenating compiler most
easily gets wrong. A statement left at the top level of a module:

```burxt
// broken.bx — a module with a mistake on its third line.
class Item { sku: String, price: Decimal<2> }

let wrong: Decimal<2> = $1.00 + 0.0825;
```

```
error: a module holds declarations, not statements: this would run when `broken.bx` was used, and a `use` is not a call
 --> broken.bx:4:1
  |
4 | let wrong: Decimal<2> = $1.00 + 0.0825;
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

`broken.bx:4`, not `buffer:117`. Without that map every error in the program would point at one enormous
file, which is the failure mode this design would otherwise have.

## Next
{: #next}

[Generics](09-generics.md) — one definition, one copy per type that uses it. Which is also what
makes `lib/option.bx` and `lib/result.bx` libraries rather than keywords.

---
title: Modules
---

# 7. Modules

One file is a module. `use` names a path.

```burxt
// lexer.bx — declarations only
class Tok { kind: Int, start: Int }
function scan(text: String) -> Int { return len(text); }
```

```burxt
// main.bx
use "lexer.bx";

region r {
    let t: Tok = Tok { kind: 7, start: 0 };
    print(scan("hello"));
}
```

```sh
$ burxt run main.bx
5
```

## The path is relative to the file that writes it

So a directory of Burxt files can be moved without editing anything inside it. There is no
search path, no include directory and no environment variable: a path that means one file on
one machine means the same file everywhere.

**Why a path rather than a name.** `use lexer;` needs a resolution rule — which directory,
which extension, which precedence — and every language that has one has a page of
documentation about it. A path is already unambiguous.

## Imports come first

`use` lines go at the top of a file, before any other item. Blank lines and comments may
sit among them; anything else ends the header.

## Everything a module declares is visible

Its functions, records, enums, interfaces and impls are all available to the file that used it.
There is no `pub` yet, and that is a deliberate deferral rather than an oversight: `pub`
doubles the annotation burden on every declaration, and its value is *hiding*, which matters
when strangers depend on your module and not before. It earns its place when a library needs
to keep an internal helper to itself.

Name collisions are already an error, which is the half of the problem that bites today.

## A module holds declarations, not statements

```burxt
// helpers.bx
print("loaded!");        // ← refused
```

```
error: a module holds declarations, not statements: this would run when `helpers.bx` was
       used, and a `use` is not a call
```

Top-level statements are what make a file a *program*. A file that runs when it is imported
is the import side-effect problem, and every language that allows it grows a convention
against it. The file you are **compiling** may of course have statements — that is what
makes it the program.

## Each file is read once, and cycles are fine

```burxt
// a.bx
use "b.bx";
function from_a() -> Int { return from_b() + 1; }

// b.bx
use "a.bx";
function from_b() -> Int { return 41; }
```

This compiles, and `from_a()` is 42. Burxt collects every declaration before checking any
body, so mutual reference across files needs no forward declarations, no header files and no
ordering rule. A file already read is not read again.

That is a real advantage over `#include`, and it falls out of the two-pass checker rather
than being designed for.

## Errors name the file they are in

The compiler reads your files into one buffer and keeps a map back to them, so a mistake
inside a used module reports *that module* and its own line number:

```
error: cannot + Decimal<2> and Decimal<4>: scales must match.
 --> bad.bx:3:12
```

The full reasoning, including what was deferred and why, is in
[`spec/M6-MODULES.md`](../../spec/M6-MODULES.md).

## Next

[Generics](09-generics.md) — one definition, one copy per type that uses it. Which is also what
makes `lib/option.bx` and `lib/result.bx` possible as libraries rather than keywords.

---
title: Getting started
---

# 1. Getting started

## Install

Burxt's compiler is written in Rust and emits native code through LLVM 18. You need both
to build it; you need neither to *run* what it produces.

```sh
sudo apt install llvm-18-dev libpolly-18-dev libzstd-dev clang-18   # Debian/Ubuntu
export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18

git clone https://github.com/andrecorugda/burxt && cd burxt
cargo install --path .        # puts `burxt` on your PATH
```

## Your first program

There is no project file, no manifest, and no build configuration. One file is a program.

```burxt
let name: String = "world";
region r {
    print("hello, " + name + "!");
}
```

```sh
$ burxt run hello.bx
hello, world!
```

Two things in three lines are worth explaining now, because they are the two things people
meet first:

**Types are written down.** `let name: String` — there is no inference on a binding. The
type you meant is the type the reader sees, and a mistake in it is a compile error rather
than a surprise three functions away.

**`region r { }` is where built values live.** Joining two Strings creates a new one, and
Burxt has no garbage collector to clean it up later, so it asks where to put it. Outside a
region, `"hello, " + name` is a compile error that tells you exactly this. See
[Memory](04-memory.md) — it is the one idea in Burxt that has no equivalent in most
languages, and it takes five minutes.

## Running

| Command | What it does |
|---|---|
| `burxt run x.bx` | Compile to native code and run it |
| `burxt build x.bx -o prog` | Compile and keep the binary |
| `burxt check x.bx` | Parse and typecheck only — no LLVM, no linker, fast |
| `burxt emit-ir x.bx` | Print the LLVM IR |
| `burxt layout x.bx` | Print record sizes, alignments and field offsets |

Use `-o` unless you want the executable in your current directory. Arguments after the
source file go to the linker unchanged: `burxt run pay.bx cside.o -lm`.

## In an editor

The VS Code extension gives syntax highlighting, diagnostics as you type, hover, and a ▶
button (`Ctrl+F5`):

```sh
python3 editors/vscode/pack.py
code --install-extension editors/vscode/burxt-0.1.2.vsix
```

It talks to `burxt lsp`, which is the same compiler you run from the terminal — so the
editor cannot disagree with the build.

## What to read next

[Numbers and money](02-numbers-and-money.md) — the reason the language exists.

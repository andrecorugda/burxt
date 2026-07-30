---
title: Getting started
---

# 1. Getting started

## What you are about to get

One file is a program. There is no manifest, no project layout, no entry point to declare and
nothing to configure. `burxt run hello.bx` produces a native executable of about 16 KB that links
nothing but libc — no runtime to ship, no VM to start.

And one command you have probably never had before: `burxt review`, which reads two versions of a
file and tells you whether the newer one **promises less** than the older one. That is further down
this page, because it is the reason the rest of the language looks the way it does.

## Install

The compiler is written in Rust and emits native code through LLVM 18. You need both to build it;
you need neither to *run* what it produces.

```sh
sudo apt install llvm-18-dev libpolly-18-dev libzstd-dev clang-18   # Debian/Ubuntu
export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18

git clone https://github.com/andrecorugda/burxt && cd burxt
cargo install --path .        # puts `burxt` on your PATH
```

No LLVM to hand? [Open the repository in a Codespace](https://codespaces.new/andrecorugda/burxt?quickstart=1)
— it builds itself on first start.

## Your first program

```burxt
let name: String = "world";
print("hello, " + name + "!");
```

```sh
$ burxt run hello.bx
hello, world!
```

That is the whole file. Two things in it are worth naming now, because they are the two things
everybody notices in the first minute.

**Types are written down.** `let name: String`, not `let name`. A binding says what it is, so the
type a reader sees is the type you meant — and a mistake in it is a compile error here rather than
a surprise three functions away. Inside a function body Burxt will infer from an obvious right-hand
side; at a boundary anyone reads, it will not.

**There is no `main`.** The file is the program, top to bottom. A five-line calculation is five
lines, which matters more than it sounds: most of what anyone writes to *check* something is
small, and a language that demands a wrapper around it discourages checking.

## The command that is not like other languages

Here is a function that will not let you overdraw an account:

```burxt
function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>
    requires amount > $0.00
    requires amount <= balance
{
    return balance - amount;
}
```

Now suppose something — a colleague in a hurry, an agent that could not satisfy the rule — removes
the second line. The body still compiles. Every test still passes; in fact the tests pass *more*,
because whatever was failing was failing on purpose. In any other language you catch that by
noticing one deleted line in a diff at 5pm on a Friday.

```sh
$ burxt review before.bx after.bx
WEAKENED  withdraw                           lost `requires amount <= balance`

1 weakened promise(s). A weakened contract is the one change that passes every test — the tests were failing BECAUSE of it.
$ echo $?
1
```

**It exits non-zero**, so it is a gate and not a report. Put it in CI and a promise cannot get
quietly smaller.

This works for one reason, and the reason is the design of the whole language: everything that
matters is in the **signature**. The scale of the money, the rounding rule, the preconditions,
what is private, what the function is allowed to reach. Nothing important hides in a body — so a
tool that reads only declarations can still tell you whether the program's promises changed.

## The loop

<svg viewBox="0 0 640 168" role="img" aria-label="write, check, run, review — and review gates the merge" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .b { fill: #fff; stroke: #111; stroke-width: 1.5; }
    .t { font: 13px ui-monospace, monospace; fill: #111; }
    .g { font: 11px ui-monospace, monospace; fill: #888; }
    .s { font: 11px ui-monospace, monospace; fill: #b00; }
    .a { stroke: #111; stroke-width: 1.5; fill: none; marker-end: url(#a1); }
    @media (prefers-color-scheme: dark) {
      .b { fill: #1b1b1b; stroke: #ddd; } .t { fill: #eee; } .s { fill: #ff8080; }
      .a { stroke: #ddd; } .g { fill: #999; }
    }
  </style>
  <defs>
    <marker id="a1" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/>
    </marker>
  </defs>

  <rect class="b" x="8" y="30" width="110" height="48" rx="4"/>
  <text class="t" x="20" y="52">hello.bx</text>
  <text class="g" x="20" y="69">you write it</text>

  <rect class="b" x="158" y="30" width="150" height="48" rx="4"/>
  <text class="t" x="170" y="52">burxt check</text>
  <text class="g" x="170" y="69">no linker, fast</text>

  <rect class="b" x="348" y="30" width="120" height="48" rx="4"/>
  <text class="t" x="360" y="52">burxt run</text>
  <text class="g" x="360" y="69">16 KB binary</text>

  <rect class="b" x="508" y="30" width="124" height="48" rx="4"/>
  <text class="t" x="520" y="52">burxt review</text>
  <text class="s" x="520" y="69">gates the merge</text>

  <path class="a" d="M118 54 L154 54"/>
  <path class="a" d="M308 54 L344 54"/>
  <path class="a" d="M468 54 L504 54"/>

  <path class="a" d="M570 82 L570 128 L67 128 L67 82"/>
  <text class="g" x="196" y="148">a weakened promise sends you back here</text>
</svg>

<div class="tablewrap" markdown="1">

| Command | What it does |
|---|---|
| `burxt run x.bx` | Compile to native code and run it |
| `burxt build x.bx -o prog` | Compile and keep the binary |
| `burxt check x.bx` | Parse and typecheck only — no LLVM, no linker, fast |
| `burxt check x.bx --json` | The same, as JSON, for an editor or a script |
| `burxt review old.bx new.bx` | What changed about what it *promises*. Non-zero if anything got weaker |
| `burxt emit-ir x.bx` | Print the LLVM IR |
| `burxt layout x.bx` | Print class sizes, alignments and field offsets |

</div>

Use `-o` unless you want the executable in your current directory. Arguments after the source file
go to the linker unchanged: `burxt run pay.bx cside.o -lm`.

## In an editor

```sh
python3 editors/vscode/pack.py
code --install-extension editors/vscode/burxt-0.1.2.vsix
```

Syntax highlighting, diagnostics as you type, hover, and a ▶ button (`Ctrl+F5`). It talks to
`burxt lsp` — the same compiler you run from the terminal, so the editor cannot disagree with the
build.

One trap worth knowing about, because it cost a full afternoon: the extension runs whichever
`burxt` binary it finds, and if you have both a `--release` and a `debug` build it prefers the
newer. After rebuilding, run **Burxt: Restart Server** from the command palette. An editor
reporting an error the terminal does not is almost always this.

## What to read next

[Numbers and money](02-numbers-and-money.md) — where being wrong costs, and the first thing the
compiler will refuse to let you do.

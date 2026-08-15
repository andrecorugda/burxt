---
layout: default
title: Install
section: install
description: Burxt needs a C compiler and nothing else. Programs it compiles need libc.
---

> **This documents Burxt 1.0.0, and it is frozen.** It will not change again — that is what makes
> it useful to someone still on 1.0. The current documentation is at
> [the top of the site]({{ site.baseurl }}/).


# Install

Linux x86-64. That is the only platform built and tested, so it is the only one offered — a
half-working download for another platform is worse than an honest absence.

```sh
sh scripts/install.sh https://github.com/andrecorugda/burxt/releases/latest/download/burxt-linux-x86_64.tar.gz
```

Or take the tarball from [the releases page](https://github.com/andrecorugda/burxt/releases) and put
the binary where you like:

```sh
tar xzf burxt-*-linux-x86_64.tar.gz
cd burxt-*
cp burxt ~/.local/bin/
```

Then:

```sh
burxt run examples/tour.bx
```

## What it needs from your machine

**A C compiler.** That is the whole list.

`burxt build` produces an object file and hands it to the system linker, so `cc` has to exist. On
Debian or Ubuntu that is `build-essential`; on macOS, `xcode-select --install`. `burxt check`
typechecks without linking anything and needs nothing at all.

**No Rust. No LLVM. No version to match.** LLVM is statically linked into the binary, which is why
it is 48 MB. That is a deliberate trade: one download now, rather than a system dependency that has
to keep matching forever. A dynamically linked build would be about 2 MB and would greet many people
with `error while loading shared libraries`, which is the commonest first-run failure for languages
that go the other way.

## What the programs you write need

**libc.** A compiled Burxt program is a native executable of about **16 KB** with nothing behind it:

```
linux-vdso.so.1    libc.so.6    ld-linux-x86-64.so.2
```

The bump allocator, the string operations and the overflow checks are emitted into every module.
There is no runtime to install, no VM to start, and nothing to keep in step with the compiler.

## The sizes, measured

<div class="tablewrap" markdown="1">

| | |
|---|---|
| The tarball | ~18 MB compressed |
| The binary, stripped | 48 MB — almost all of it LLVM |
| A compiled program | ~16 KB |

</div>

## Without installing anything

[Open it in a Codespace](https://codespaces.new/andrecorugda/burxt?quickstart=1) — a browser, the
real compiler, and the editor extension with live diagnostics on real code.

## Building from source

Only needed to work *on* the compiler. This is the one path that wants a toolchain:

```sh
git clone https://github.com/andrecorugda/burxt
cd burxt
cargo build --release            # needs Rust and LLVM 18
cargo test                       # 34 invariants, including the byte-identical fixpoint
```

LLVM **18** exactly — the binding is feature-gated to it, so a newer one will not link.
`.cargo/config.toml` points at `/usr/lib/llvm-18`, which is where `apt.llvm.org`'s script puts it.

`sh scripts/release.sh` builds the tarball and then unpacks its own output into a scratch directory
and compiles a program with the *unpacked* binary before reporting success — so a broken artifact
fails at build time rather than on somebody else's machine.

## Editor support

The tarball ships the compiler; the extension is a separate asset on the same release. It gives
syntax highlighting and live diagnostics from the compiler itself, not a reimplementation of its
rules:

```sh
code --install-extension burxt-*.vsix
```

---
layout: doc
title: The command line
section: reference
description: "Every burxt command, including review and mcp-schema."
---


# The command line

Read out of the usage block in `src/rust-compiler/main.rs`, so this cannot list a command the compiler does not have. Two of them exist nowhere else in programming: `burxt review`, which answers what a change did to what a program **promises**, and `burxt mcp-schema`, which derives an MCP tool schema from the preconditions so the two cannot drift. Both are explained in [Tools and agents](../guide/12-tools-and-agents.html).

| Command | Takes | What it does |
|---|---|---|
| `burxt check` | `<file.bx>` | parse and typecheck only |
| `burxt lsp` | — | language server over stdio |
| `burxt fetch` | — | get the dependencies, write burxt.lock |
| `burxt build` | `<file.bx> [link args...]` | compile to a native executable |
| `burxt run` | `<file.bx> [link args...]` | compile then run |
| `burxt emit-ir` | `<file.bx> [--target ...]` | print LLVM IR |
| `burxt layout` | `<file.bx>` | print class layouts |
| `burxt explain` | `memory <file.bx>` | what each function builds |
| `burxt review` | `<old.bx> <new.bx>` | what changed about what it PROMISES |
| `burxt mcp-schema` | `<file.bx>` | the MCP tool manifest, from the preconditions |

## Flags

Scraped from the same usage block. Flags may be written before or after the file, so `burxt build -O0 -g pay.bx -o pay` and `burxt build pay.bx -O0 -g -o pay` are the same command.

| Flag | What it does |
|---|---|
| `-o <path>` | where to write the executable (default ./<name>) |
| `-g` | emit DWARF debug info: a line table, and every parameter and `let` with its name, type and stack slot. A debugger can then stop on a line and read a local — which is the alternative to inserting a `print`, and a `print` MOVES THE STACK and can change the answer. Off by default: debug info carries absolute paths and a producer string, so it would make the emitted IR differ between machines. |
| `-O0` | do not optimise. Independent of -g on purpose: -O2 -g is for a crash report from the field, -O0 -g is for stepping. Use both to follow a program statement by statement. |
| `--target <triple>` | build for another machine, e.g. aarch64-apple-darwin. Emits an OBJECT and stops: linking needs that target's libc and linker, so it is left to that target's toolchain. The emitted IR is identical for every target, which is what makes the decimal answers identical too. burxt review --semver <old.bx> <new.bx> [--require patch|minor|major] A stricter `requires` is a MAJOR — it promises more and breaks callers. A public function that gains an effect is a major too, because effects propagate and every caller must declare it. It reads the interface, not the behaviour: it can prove a change is AT LEAST a major, never that an upgrade is safe. |

Arguments after the source file that are not flags go to the linker unchanged, so `burxt run pay.bx cside.o -lm` links the C you call.


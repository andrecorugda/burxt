---
layout: doc
title: The command line
section: reference
description: Every burxt command, including review and mcp-schema.
---


# The command line

Read out of the usage block in `src/main.rs`, so this cannot list a command the compiler does not have. Two of them exist nowhere else in programming: `burxt review`, which answers what a change did to what a program **promises**, and `burxt mcp-schema`, which derives an MCP tool schema from the preconditions so the two cannot drift. Both are explained in [Tools and agents](../guide/12-tools-and-agents.html).

| Command | Takes | What it does |
|---|---|---|
| `burxt check` | `<file.bx>` | parse and typecheck only |
| `burxt lsp` | — | language server over stdio |
| `burxt build` | `<file.bx> [link args...]` | compile to a native executable |
| `burxt run` | `<file.bx> [link args...]` | compile then run |
| `burxt emit-ir` | `<file.bx>` | print LLVM IR |
| `burxt layout` | `<file.bx>` | print class layouts |
| `burxt review` | `<old.bx> <new.bx>          what changed about what it PROMISES` | burxt mcp-schema <file.bx>               the MCP tool manifest, from the preconditions |
| `burxt mcp-schema` | `<file.bx>` | the MCP tool manifest, from the preconditions |

Arguments after the source file go to the linker unchanged, so `burxt run pay.bx cside.o -lm` links the C you call. `-o <path>` says where the executable goes.


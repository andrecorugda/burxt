---
title: What Burxt promises between versions
---

# What Burxt promises between versions

From **1.0.0**, this is the promise. Before it, there was none — the version was a count of
commits, and the language changed under people who were not there yet.

Most compatibility promises are a paragraph of intent. This one has a command behind it.

---

## The three numbers

Ordinary semantic versioning, stated so nothing is assumed:

| bump | means |
|---|---|
| **major** — `2.0.0` | a program that compiled may stop compiling, or may compile and mean something different |
| **minor** — `1.1.0` | new things exist; everything that compiled still compiles and still means the same |
| **patch** — `1.0.1` | something was wrong and is now right; no new surface |

A **patch adds nothing.** Not a function, not a keyword, not a flag. If it adds anything, it is a
minor, and calling it a patch would break the only rule a version number carries.

---

## The command

Every other language asks you to classify a change by judgement. Burxt can be asked:

```
$ burxt review --semver before.bx after.bx

major   `withdraw` gained `requires amount <= balance` — a caller that satisfied
        the old signature may not satisfy this one
major   `read_config` now touches files — effects propagate, so every caller
        must declare it too or stop compiling
minor   `extra` is new and public

minimum bump: major
```

This works because of one decision made long before anyone thought about versioning: **a Burxt
signature carries what a function promises.** `requires`, `ensures`, `touches`, and — since 1.0 —
`public`. All four are machine-readable, so a change to any of them is a diff a compiler can take.

`cargo-semver-checks` is the closest thing elsewhere, and it compares **types**. It cannot see that a
precondition got stricter, because in Rust the precondition is in a doc comment or in nobody's head.

### The two rules that surprise people

**A stricter `requires` is a MAJOR.** It promises *more*, and it breaks every caller that satisfied
the old signature. This is the mirror of the mistake `burxt review` was built to catch: an agent
deleting a precondition to make a test pass. Deleting one is a lie about your own code; adding one
is a break in somebody else's.

**A public function that gains an effect is a MAJOR.** Effects propagate, so a function that starts
touching `files` forces every caller to write `touches files` in its own signature or stop
compiling. In a language where effects are not in the type, that same change is invisible and ships
as a patch.

### The gate

```
burxt review --semver before.bx after.bx --require minor
```

Exits non-zero when the bump you claimed is smaller than the one the interface demands. That is a CI
check, and it is deliberately **not** a build gate: a compiler that refuses to compile because a
version *string* is wrong is enforcing policy rather than correctness, and this compiler only ever
refuses things that are wrong.

---

## What the command cannot do, stated here rather than in a footnote

**It reads the interface. It does not read the behaviour.**

A function whose signature, contracts and effects are all unchanged, and which now returns different
numbers, is a breaking change that nothing here detects.

So:

> `burxt review --semver` sets the **minimum**. It can prove a change is *at least* a major, and it
> can prove *nothing in the interface broke*. **It can never prove that an upgrade is safe.**
>
> A person may always choose a higher bump than the tool says. Never a lower one.

That is also why a release may be a major for reasons no tool can see — a milestone, a rewritten
runtime, a changed default. The number is a promise made by people; the tool stops that promise from
being accidentally too small.

---

## What is covered

Everything a program outside this repository can name:

- the syntax the compiler accepts
- the standard library in `lib/`, its function names, signatures and contracts
- `public` declarations in a package you depend on
- the command line: `check`, `build`, `run`, `fetch`, `review`, `mcp-schema`, and their flags
- the manifest and lockfile formats
- exit codes: **70** for a named runtime failure, **0** for success

## What is not

- **anything not declared `public` in a package.** That is the point of the keyword: a package's
  own helpers are its business, and it may change them in a patch.
- **the exact wording of a diagnostic.** The message will keep improving. Do not parse it; the exit
  code and the JSON from `--json` are the stable surface.
- **the emitted LLVM IR.** It is byte-identical across targets *within* a version, which is a
  guarantee about determinism, not about the next version.
- **the compiler's own internals**, `src/burxt-compiler/` included. It is written in Burxt and it is
  a program that happens to live here, not an interface.
- **anything marked in [what Burxt does not do](limitations.html) as *not yet***. A gap being filled
  is a minor; a gap's *workaround* becoming unnecessary is not a break.

---

## The honest part

A compatibility promise is only worth what the project does the first time keeping it is expensive.
This one has not been tested yet — 1.0.0 is where it starts.

What can be said today is narrower and checkable: the tool that enforces it exists, it runs in CI,
and it is wired to the same contracts the compiler already checks at runtime. **The promise and the
enforcement are the same sentence in the same file**, which is the only arrangement that cannot
drift.

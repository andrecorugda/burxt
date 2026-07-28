# Negative inputs — wrong on purpose

Every `.bx` file in this directory **fails to compile, deliberately**. That is the whole
point of the directory, and it is why the directory exists instead of a naming convention:
a folder called `negative/` answers the question before anyone opens a file, and a rule
holds for files nobody has written yet while a list of exceptions rots.

They are **data**, read by the self-hosted examples one level up. A checker with nothing
to catch demonstrates nothing.

| File | Read by | The mistakes in it |
|---|---|---|
| `money.bx` | [`examples/checker.bx`](../checker.bx) | Six bindings: three legal, three the scale rule refuses — adding `Decimal<2>` to `Decimal<4>`, a mixed-scale product with no rounding contract, and a `Decimal<2>` bound to an `Int`. |
| `sample.bx` | [`examples/lexer.bx`](../lexer.bx) | One byte the language does not know (`@`), so the lexer has a real diagnostic to build and report. |
| `declarations.bx` | [`examples/symbols.bx`](../symbols.bx) | A name declared twice. Burxt has no shadowing, so the symbol table has a redeclaration to catch — which is the pass's whole reason to exist. |
| `declarations.bx` | [`examples/symbols.bx`](../symbols.bx) | A name declared twice. Burxt has no shadowing, so the symbol table has a redeclaration to catch — which is the whole reason that pass exists. |

Running one directly does what it says on the tin:

```sh
$ burxt run examples/negative/money.bx
error: cannot + Decimal<2> and Decimal<4>: scales must match. Burxt does not silently
       rescale money.
```

What you probably want is the example that **reads** it — the same mistakes, found by a
checker written in Burxt:

```sh
$ burxt run examples/checker.bx
let broken : Decimal<2>
  cannot apply `+` to Decimal<2> and Decimal<4>: addition combines like quantities, so
  the scales must match
...
--- checked 8 declarations
```

**Your editor will squiggle these files, and it is right to.** The language server checks
every `.bx` file you open and cannot know a directory is negative by convention. The name
is for you; the test below is for the repository.

## The intent is machine-checked

`the_negative_examples_are_still_negative` asserts that **every** file here is
rejected, and that every file in [`../inputs/`](../inputs) compiles. If someone "fixes" one
of these, the suite says so — otherwise a demonstration quietly becomes a demonstration of
nothing, with all tests green.

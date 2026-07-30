---
title: The C boundary
---

# 7. The C boundary

## The problem, as it actually arrives

Here is a stack that is exact everywhere:

```
NUMERIC(12,2) in Postgres      exact
Decimal in the application     exact
```

And here is the function call between them, in the driver, three layers down where nobody looks:

```c
int charge(const char *account, double amount);
```

`19.99` is not representable in binary floating point. It never was. So the amount that leaves your
exact application and arrives at your exact database went through a value that was *approximately*
19.99, and whether that costs you a cent depends on which way the last multiplication happened to
round.

**Guarding the arithmetic and then abandoning the boundary guards nothing.** Real financial defects
overwhelmingly live at boundaries rather than in arithmetic
([the design record](../../spec/N1-BOUNDARY-EXACTNESS.md)), and this page is the wall Burxt puts
there.

Which is also why it exists at all: a language that cannot call C is a language you cannot deploy.

```burxt
external function strlen(s: String) -> Int;
```

```sh
cc -c mylib.c -o mylib.o
burxt run app.bx mylib.o -lm      # arguments after the file go to the linker unchanged
```

## Think of customs

Everything crossing gets declared, and the declaration says *how* it is packed. Money is counted in
whole units — never converted at a rate nobody wrote down.

<svg viewBox="0 0 640 252" role="img" aria-label="A Decimal crosses to C as its exact integer; as a double it does not cross at all" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .b { fill: #fff; stroke: #111; stroke-width: 1.5; }
    .wall { stroke: #111; stroke-width: 2.5; }
    .t { font: 13px ui-monospace, monospace; fill: #111; }
    .g { font: 11px ui-monospace, monospace; fill: #888; }
    .s { font: 11px ui-monospace, monospace; fill: #b00; }
    .a { stroke: #111; stroke-width: 1.5; fill: none; marker-end: url(#a7); }
    .x { stroke: #b00; stroke-width: 2; }
    @media (prefers-color-scheme: dark) {
      .b { fill: #1b1b1b; stroke: #ddd; } .wall { stroke: #ddd; } .t { fill: #eee; }
      .s { fill: #ff8080; } .a { stroke: #ddd; } .g { fill: #999; } .x { stroke: #ff8080; }
    }
  </style>
  <defs>
    <marker id="a7" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/>
    </marker>
  </defs>

  <text class="g" x="20" y="20">Burxt</text>
  <text class="g" x="596" y="20">C</text>
  <line class="wall" x1="316" y1="24" x2="316" y2="60"/>
  <line class="wall" x1="316" y1="108" x2="316" y2="238"/>

  <rect class="b" x="20" y="62" width="110" height="44" rx="4"/>
  <text class="t" x="34" y="90">$19.99</text>
  <path class="a" d="M130 84 L474 84"/>
  <text class="s" x="186" y="76">as scaled</text>
  <rect class="b" x="486" y="62" width="134" height="44" rx="4"/>
  <text class="t" x="500" y="82">1999</text>
  <text class="g" x="500" y="98">int64_t, exact</text>

  <rect class="b" x="20" y="150" width="110" height="44" rx="4"/>
  <text class="t" x="34" y="178">$19.99</text>
  <path class="a" d="M130 172 L292 172"/>
  <text class="s" x="146" y="164">as a double</text>
  <path class="x" d="M300 165 L314 179"/><path class="x" d="M314 165 L300 179"/>
  <text class="s" x="340" y="170">19.989999999999998</text>
  <text class="g" x="340" y="188">not the same money</text>

  <text class="g" x="20" y="232">nothing that returns a pointer crosses at all — the wall below</text>
</svg>

## Money crosses exactly, or not at all

```burxt
external function cents_doubled(amount: Decimal<2> as scaled) -> Int;
```

`as scaled` is a **marshaller**, and it is mandatory. The `Decimal<2>` crosses as the exact integer
it already is: `$19.99` arrives in C as `1999`. Leave it off and the compiler will not guess:

```
error: in external function `pay`, parameter `amount` is Decimal<2> and C has no decimal type,
       so the crossing has to say how the value is encoded. Declare `amount: Decimal<2> as
       scaled` to pass the exact unscaled integer (C receives it scaled by 10^2), or take a
       String and pass `to_string(amount)` as text.
```

`scaled` is the **only** marshaller. There is no way to spell "send it as a double", and handing a
`Decimal` to a `CDouble` parameter is refused with the reason rather than the rule:

```
error: a C `double` cannot hold Decimal<2> exactly — a value like 0.10 is not representable in
       binary floating point, so this crossing would silently change the amount. Declare the
       parameter of `take` as `Decimal<2> as scaled` to pass the exact unscaled integer (C
       receives it scaled by 10^2), or take a String and pass `to_string(...)` as text.
```

Nor can a C function *return* one, for the same reason from the other direction:

```
error: external function `rate` returns CDouble, but Burxt has no float type to receive it
       exactly — a double cannot hold most decimal amounts. Have the C function return the
       scaled integer (declare `-> Int`), or return it as text.
```

## `CInt` and `CDouble`

C's widths. They exist only in `external function` signatures — a Burxt caller passes and receives
an ordinary `Int`. Declaring `-> CInt` matters when the C function returns a 32-bit `int` and you
want the sign to survive.

An `Int` may cross into a `CDouble` parameter, because that is often exactly what a C math function
wants. It is checked, at run time, at the point a double stops being exact:

```
burxt runtime error: this Int cannot cross as a C double exactly — a double represents every
integer only up to 2^53
```

## The pointer wall

```burxt
external function getenv(name: String) -> String;
```

```
error: external function `getenv` returns String, but only Int or CInt may cross the C
       boundary as a return for now — Burxt cannot yet track who owns memory a C function
       returns. (If the C function returns a 32-bit `int`, declare `-> CInt` so the sign
       survives.)
```

**Anything returning a pointer is out of reach directly**: `getenv`, `opendir`, `fopen`. That is not
an oversight, it is [the memory model](04-memory.md). Burxt can say where every value it owns lives.
It cannot say that about storage a C library allocated on terms it was never told.

Two ways around it today, both honest about what they cost:

1. **Launder it through `Int`.** `external function fopen(path: String, mode: String) -> Int;`
   works — a pointer is an integer in a register — and the claim that the handle is still valid is
   now *yours*, not the compiler's.
2. **Write a C wrapper** that returns an int and takes the handle as one. The compiler tells you
   when you need this: a symbol the Burxt runtime itself uses (`fputs`, `printf`) has to be called
   through a differently-named wrapper.

Sockets are a happy accident of this rule: a file descriptor **is** an int, so `socket`, `connect`,
`send` and `recv` cross the wall unchanged.

The standard library will wrap this once, correctly, so that reading a directory is `fs.list(path)`
rather than a pointer you promised about.

## An extern is where an effect enters the program

There is no body for the compiler to read, so the declaration is the only thing that can say what a
C function reaches:

```burxt
external function system(command: String) -> CInt touches commands;
external function time(unused: Int) -> Int touches clock;
```

An extern that declares nothing is **taken at its word** — right for `strlen`, and a lie for
`system`, which is why the standard library declares its own. See [Effects](06-effects.md), and the
four surprises that turning this on found in a nine-function module.

## What reaches the OS today

Everything whose signature is ints and strings in, an int out — which is more than it sounds:

```burxt
external function system(command: String) -> CInt touches commands;
external function mkdir(path: String, mode: Int) -> CInt touches files;
external function rename(from: String, to: String) -> CInt touches files;
external function remove(path: String) -> CInt touches files;
external function getchar() -> CInt touches input;
external function time(nothing: Int) -> Int touches clock;
```

Plus the builtins that need no FFI at all: `read_file`, `write_file`, `argument`,
`argument_count`, `print`.

[`examples/ffi.bx`](../../examples/ffi.bx) and its `ffi.c` are all of this as one program you can
link and run.

## Next

[Modules](08-modules.md) — `use`, one file per module, and what that means for privacy.

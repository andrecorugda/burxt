---
title: The C boundary
---

> **This documents Burxt 1.0.0, and it is frozen.** It will not change again — that is what makes
> it useful to someone still on 1.0. The current documentation is at
> [the top of the site]({{ site.baseurl }}/).


# 7. The C boundary

## What this is for
{: #what-this-is-for}

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
([the design record](https://github.com/andrecorugda/burxt/blob/main/spec/1.0/N1-BOUNDARY-EXACTNESS.md)), and this page is the wall Burxt puts
there.

Which is also why it exists at all: a language that cannot call C is a language you cannot deploy.

```burxt
external function strlen(s: String) -> Int;
```

```sh
cc -c mylib.c -o mylib.o
burxt run app.bx mylib.o -lm      # arguments after the file go to the linker unchanged
```

## Think of a customs desk
{: #think-of-a-customs-desk}

At customs, everything crossing gets declared, and the declaration says *how it is packed*. Cash is
counted in whole notes. Nobody converts it at a rate they made up on the spot, and nothing goes through
undeclared because it looked harmless.

<figure>
<svg viewBox="0 0 680 272" role="img" aria-label="A customs desk: a Decimal declared as scaled crosses as its exact integer, a Decimal sent as a C double is refused, and a pointer cannot cross at all" style="max-width:100%;height:auto;">
  <style>
    .desk { fill: #f5f5f7; stroke: #1d1d1f; stroke-width: 2; }
    .wall { stroke: #1d1d1f; stroke-width: 3; }
    .box  { fill: #ffffff; stroke: #1d1d1f; stroke-width: 1.6; }
    .okb  { fill: #ffffff; stroke: #0f6f3c; stroke-width: 1.8; }
    .bad  { fill: #ffffff; stroke: #c8102e; stroke-width: 1.8; stroke-dasharray: 5 4; }
    .pass { fill: none; stroke: #0f6f3c; stroke-width: 2; marker-end: url(#mp); }
    .stop { fill: none; stroke: #c8102e; stroke-width: 2; marker-end: url(#ms); }
    .no   { fill: none; stroke: #c8102e; stroke-width: 2; }
    .h    { font: 600 13.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .t    { font: 11.5px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #1d1d1f; }
    .grn  { font: 600 11.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #0f6f3c; }
    .red  { font: 600 11.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #c8102e; }
    .cap  { font: 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
  </style>
  <defs>
    <marker id="mp" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="#0f6f3c"/>
    </marker>
    <marker id="ms" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="#c8102e"/>
    </marker>
  </defs>

  <text class="h" x="8" y="18">Burxt</text>
  <text class="h" x="540" y="18">C</text>

  <line class="wall" x1="340" y1="26" x2="340" y2="214"/>
  <rect class="desk" x="316" y="96" width="48" height="56" rx="6"/>

  <!-- declared as scaled: crosses exactly -->
  <rect class="okb" x="14" y="38" width="150" height="34" rx="6"/>
  <text class="t" x="24" y="60">$19.99 as scaled</text>
  <path class="pass" d="M172 55 h140 M368 55 h48"/>
  <rect class="okb" x="424" y="38" width="130" height="34" rx="6"/>
  <text class="t" x="434" y="60">1999  (int64)</text>
  <text class="grn" x="424" y="88">exact, and C is told</text>
  <text class="grn" x="424" y="104">it is scaled by 100</text>

  <!-- as a double: refused -->
  <rect class="bad" x="14" y="120" width="150" height="34" rx="6"/>
  <text class="t" x="24" y="142">$19.99 as CDouble</text>
  <path class="stop" d="M172 137 h116"/>
  <g class="no">
    <circle cx="302" cy="137" r="12"/>
    <line x1="294" y1="129" x2="310" y2="145"/>
  </g>
  <text class="red" x="376" y="133">refused: 0.10 is not</text>
  <text class="red" x="376" y="149">representable in binary</text>
  <text class="red" x="376" y="165">floating point</text>

  <!-- a pointer: cannot cross -->
  <rect class="bad" x="14" y="178" width="150" height="34" rx="6"/>
  <text class="t" x="24" y="200">char* back from C</text>
  <path class="stop" d="M172 195 h116"/>
  <g class="no">
    <circle cx="302" cy="195" r="12"/>
    <line x1="294" y1="187" x2="310" y2="203"/>
  </g>
  <text class="red" x="376" y="199">refused: nobody can say who owns it</text>

  <text class="cap" x="8" y="246">Only <tspan font-family="ui-monospace, monospace">Int</tspan> and <tspan font-family="ui-monospace, monospace">CInt</tspan> may come back today. Everything that crosses says how it is packed,</text>
  <text class="cap" x="8" y="264">and a crossing that would change the value is a compile error rather than a rounding.</text>
</svg>
<figcaption>Guarding the arithmetic and then handing the value to a <code>double</code> guards nothing. This
is the edge where every other stack loses the cent.</figcaption>
</figure>

## A step closer
{: #a-step-closer}

Money is counted in whole units — never converted at a rate nobody wrote down.

<svg viewBox="0 0 640 252" role="img" aria-label="A Decimal crosses to C as its exact integer; as a double it does not cross at all" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .b { fill: #fff; stroke: #1d1d1f; stroke-width: 1.5; }
    .wall { stroke: #1d1d1f; stroke-width: 2.5; }
    .t { font: 13px ui-monospace, monospace; fill: #1d1d1f; }
    .g { font: 11px ui-monospace, monospace; fill: #3a3a3e; }
    .s { font: 11px ui-monospace, monospace; fill: #c8102e; }
    .a { stroke: #1d1d1f; stroke-width: 1.5; fill: none; marker-end: url(#a7); }
    .x { stroke: #c8102e; stroke-width: 2; }
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

## In code
{: #in-code}

### Money crosses exactly, or not at all

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

### `CInt` and `CDouble`

C's widths. They exist only in `external function` signatures — a Burxt caller passes and receives
an ordinary `Int`. Declaring `-> CInt` matters when the C function returns a 32-bit `int` and you
want the sign to survive.

An `Int` may cross into a `CDouble` parameter, because that is often exactly what a C math function
wants. It is checked, at run time, at the point a double stops being exact:

```
burxt runtime error: this Int cannot cross as a C double exactly — a double represents every
integer only up to 2^53
```

### The pointer wall

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

### An extern is where an effect enters the program

There is no body for the compiler to read, so the declaration is the only thing that can say what a
C function reaches:

```burxt
external function system(command: String) -> CInt touches commands;
external function time(unused: Int) -> Int touches clock;
```

An extern that declares nothing is **taken at its word** — right for `strlen`, and a lie for
`system`, which is why the standard library declares its own. See [Effects](06-effects.md), and the
four surprises that turning this on found in a nine-function module.

## Why it is built this way
{: #why-it-is-built-this-way}

**A guard that stops at the boundary is not a guard.** This is the whole argument. A language can be
scrupulous about `Decimal<2>` for four hundred lines and then hand the value to a C function taking a
`double`, and every one of those four hundred lines was decoration. Real financial defects live at
boundaries far more often than in arithmetic, because the arithmetic is the part people check.

**`as scaled` sends the integer, which is what the value actually is.** A `Decimal<2>` *is* an integer
and a scale. Sending the integer is not a conversion at all — it is the value, and C is told the scale.

**The pointer wall is an admission, and admissions are cheaper than guesses.** Burxt cannot yet track who
owns memory a C function returns, so it refuses to let one come back rather than picking an owner and
hoping. A wrong guess there is a use-after-free, which is the failure mode
[the memory model](04-memory.md) exists to make impossible.

**An `external function` is where an effect enters the program.** It has no body, so nothing can be
inferred from it — which is exactly why `touches` and `allocates` are *required* on one. Everywhere else
they are checked; here they are the only source of truth.

## What it costs
{: #what-it-costs}

**Only `Int` and `CInt` may come back.** No strings, no structs, no pointers. That rules out a large
share of real C APIs today.

**`as scaled` means C has to know the scale.** You are passing `1999` and a convention. Get the
convention wrong on the C side and the guard on this side did not help — it moved the mistake somewhere
a reviewer can see it, which is better, but it did not remove it.

**`CDouble` exists and is genuinely dangerous.** It is there for C APIs that take a double and mean it.
Crossing an inexact value through one exits 70 with a named error rather than rounding — but the type is
available, and using it for money is a decision you can make.

**You write the `touches` and `allocates` yourself on an extern**, and nothing can check you. This is the
one place in the language where an annotation is trusted rather than verified.

### What reaches the OS today

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

[`examples/ffi.bx`](https://github.com/andrecorugda/burxt/blob/main/examples/ffi.bx) and its `ffi.c` are all of this as one program you can
link and run.

## When you reach for it
{: #when-you-reach-for-it}

<div class="tablewrap" markdown="1">

| You need to pass | Declare it |
|---|---|
| a count, a flag, a file descriptor | `Int`, or `CInt` if C says `int` and the sign matters |
| a path, a command, any text | `String` |
| money, to a C function that understands a scaled integer | `Decimal<2> as scaled` |
| money, to a C function that only takes a double | take a `String` instead and pass `to_string(...)` |
| a genuine floating-point measurement | `CDouble` — and be sure it is not money |
| anything that returns a pointer | not yet. Wrap it in C so the wrapper returns an `Int` |

</div>

And on every `external function`: say what it `touches`, and say `allocates` if it builds anything.
Neither can be inferred, because there is no body to look at.

## Examples
{: #examples}

**A real crossing, running.** [`examples/ffi.bx`](https://github.com/andrecorugda/burxt/blob/main/examples/ffi.bx)
with its `ffi.c`, linked and run:

```sh
$ burxt run examples/ffi.bx examples/ffi.c
16
C says: hello from Burxt
3998
```

That last number is the point: `$19.99 as scaled` arrived in C as `1999`, was doubled there, and came
back as `3998` — with every digit, through a boundary that in most stacks is where the cent goes missing.

**The refusal that makes it worth trusting.** Sending money as a `double`:

```burxt
external function take_double(n: CDouble) -> Int touches commands;

let price: Decimal<2> = $19.99;
print(take_double(price));
```

```
error: a C `double` cannot hold Decimal<2> exactly — a value like 0.10 is not representable in binary floating point, so this crossing would silently change the amount. Declare the parameter of `take_double` as `Decimal<2> as scaled` to pass the exact unscaled integer (C receives it scaled by 10^2), or take a String and pass `to_string(...)` as text.
 --> pay.bx:4:19
  |
4 | print(take_double(price));
  |                   ^^^^^
```

**And the pointer wall**, which refuses the declaration itself rather than waiting for a call:

```burxt
external function strdup(s: String) -> String touches commands;
```

```
error: external function `strdup` returns String, but only Int or CInt may cross the C boundary as a return for now — Burxt cannot yet track who owns memory a C function returns. (If the C function returns a 32-bit `int`, declare `-> CInt` so the sign survives.)
 --> c.bx:1:1
  |
1 | external function strdup(s: String) -> String touches commands;
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Note that it says **for now**, and says why. A refusal that explains what the compiler cannot yet do is
a different thing from one that pretends the design forbids it.

## Next
{: #next}

[Modules](08-modules.md) — `use`, one file per module, and what that means for privacy.

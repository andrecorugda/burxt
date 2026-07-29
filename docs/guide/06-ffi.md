# 6. The C boundary

Burxt calls C, because a language that cannot is a language you cannot deploy. What crosses
is deliberately narrow.

```burxt
external function strlen(s: String) -> Int;
external function shout(text: String) -> CInt;
```

```sh
cc -c mylib.c -o mylib.o
burxt run app.bx mylib.o -lm      # arguments after the file go to the linker unchanged
```

## Money crosses exactly, or not at all

```burxt
external function cents_doubled(amount: Decimal<2> as scaled) -> Int;
```

`as scaled` is a **marshaller**, and it is mandatory. A `Decimal<2>` crosses as the exact
integer it already is: `$19.99` arrives in C as `1999`.

Handing C a `double` instead is a compile error:

```
error: a Decimal cannot cross as CDouble: binary floating point cannot hold a decimal
       exactly, which is the whole reason this type exists
```

That boundary is where exactness is normally lost in every other stack — an exact decimal in
the database, an exact decimal in the application, and a `double` in the function call
between them.

An `Int` crossing to a `CDouble` **is** allowed, and range-checked at 2^53, because beyond
that a double cannot hold an integer exactly either.

## `CInt` and `CDouble`

C's widths, and they exist only in `external function` signatures. A Burxt caller passes and
receives an `Int`; declaring `-> CInt` matters when the C function returns a 32-bit `int`
and you want the sign to survive.

## The pointer wall

```burxt
external function getenv(name: String) -> String;
```

```
error: only Int or CInt may cross the C boundary as a return for now — Burxt cannot yet
       track who owns memory a C function returns
```

**Anything returning a pointer is out of reach directly**: `getenv`, `opendir`, `fopen`,
sockets. This is not an oversight, it is the memory model: Burxt describes where every value
lives, and it cannot describe storage a C library allocated on terms it does not know.

Two ways around it today, both honest about what they cost:

1. **Launder it through `Int`.** `external function fopen(path: String, mode: String) -> Int;` works
   — a pointer is an integer in a register — and the claim that the handle is valid is
   *yours*, not the compiler's.
2. **Write a C wrapper** that returns an int and takes the handle as one. The compiler will
   tell you when you need this: a symbol the Burxt runtime itself uses (`fputs`, `printf`)
   must be called through a differently-named wrapper.

The standard library will wrap this once, correctly, so that reading a directory is
`fs.list(path)` rather than a pointer you promised about.

## What reaches the OS today

Everything whose signature is ints and strings in, an int out — which is more than it
sounds:

```burxt
external function system(command: String) -> CInt;   // run a command
external function mkdir(path: String, mode: Int) -> CInt;
external function rename(from: String, to: String) -> CInt;
external function remove(path: String) -> CInt;
external function getchar() -> CInt;                 // read stdin, a byte at a time
external function time(nothing: Int) -> Int;
```

Plus the builtins that need no FFI at all: `read_file`, `write_file`, `argument`, `argument_count`,
`print`.

## Reference

[Every keyword, builtin, operator and error](reference.md).

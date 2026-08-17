---
layout: default
title: How do I…?
section: how-do-i
description: "Short answers to the things people actually want to do — the program you write, and what it prints."
---

# How do I…?

Short answers. Each one is a program you can paste into a file and run, and the output shown is what
it prints. Nothing here needs the pages before it — jump to what you want.

Save any of these as `try.bx` and run it:

```sh
burxt run try.bx
```

## …print something

There is no entry point to declare. A file is its statements, top to bottom.

```burxt
print("hello");
```

```
hello
```

## …read a file that might not be there

`file_read_maybe` answers `Some` with the contents or `None` — for missing, unreadable, or a
directory. It never stops your program, and you have to say what happens in both cases.

```burxt
use "lib/files.bx";

match file_read_maybe("notes.txt") {
    Some(text) => { print("read " + to_string(len(text)) + " bytes"); }
    None => { print("no notes.txt here"); }
}
```

```
no notes.txt here
```

**There is no null.** `None` is a value you have to open, so the case where the file is missing
cannot be the case you forgot.

## …take an argument from the command line

```burxt
use "lib/os.bx";

if os_arg_count() > 1 {
    print("hello, " + os_arg(1));
} else {
    print("usage: try.bx <name>");
}
```

```
usage: try.bx <name>
```

## …handle something that can fail

`Result` carries either the answer or the reason. Same shape as `Option`, one more piece of
information.

```burxt
use "lib/result.bx";

function half(n: Int) -> Result<Int, String> {
    if n < 0 {
        return Result.Error("negative: " + to_string(n));
    }
    return Result.Ok(divide_toward_zero(n, 2));
}

match half(0 - 7) {
    Ok(v) => { print("half is " + to_string(v)); }
    Error(why) => { print("refused — " + why); }
}
```

```
refused — negative: -7
```

## …work with money without it drifting

Money is not a float. Write it with a `$` and it keeps its scale exactly.

```burxt
let price: Decimal<2> = $19.99;
let qty: Int = 3;
print(to_string(price * qty));
```

```
59.97
```

**Now try to lose a penny.** Multiply two two-place amounts and the exact answer has four places —
so the compiler asks what you want rather than rounding for you:

```burxt
let rate: Decimal<2> = $0.07;
let amount: Decimal<2> = $19.99;
let tax: Decimal<2> = amount * rate;
```

```
error: this multiplication of Decimal<2> by Decimal<2> has an exact product with 4 decimal
places, and reaching Decimal<2> means rounding it. Say how — Decimal<2, RoundHalfEven> —
or take the exact answer with Decimal<4>.
```

That is the whole idea of the language in one message. It is not being difficult — it is refusing to
pick a rounding rule on your behalf, because that is a decision with money attached.

## …divide two whole numbers

`/` on two `Int`s does not compile, and the message says why:

```
error: `/` on two Ints would have to round, and one operator cannot say which way: -7 divided
by 2 is -3 rounding toward zero and -4 rounding down. Say which you mean —
`divide_floor(a, b)`, `divide_toward_zero(a, b)`, or `remainder(a, b)` for the remainder.
```

So you pick:

```burxt
print(to_string(divide_floor(7, 2)));
```

```
3
```

The two answers differ only for negative numbers, which is exactly when nobody checks. You will
meet this in your first ten minutes; it is the same idea as the money rule above.

## …loop over a list

```burxt
let names: [String] = ["ada", "grace", "alan"];
let mutable i: Int = 0;
while i < len(names) {
    print(names[i]);
    i = i + 1;
}
```

```
ada
grace
alan
```

## …group data together

```burxt
class Point {
    x: Int,
    y: Int,

    function (self) shown() -> String {
        return "(" + to_string(self.x) + ", " + to_string(self.y) + ")";
    }
}

let p: Point = Point { x: 3, y: 4 };
print(p.shown());
```

```
(3, 4)
```

## …stop a function being called with nonsense

Put the rule in the signature, where a reviewer sees it without reading the body.

```burxt
function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>
    requires amount > $0.00
    requires amount <= balance
{
    return balance - amount;
}

print(to_string(withdraw($100.00, $30.00)));
```

```
70.00
```

Call it with more than the balance and it stops, naming the rule that was broken:

```
burxt runtime error: `requires amount <= balance` failed in `withdraw`
```

Not a stack trace three functions away from the mistake — the sentence you wrote, quoted back.

## …say what a function is allowed to touch

```burxt
use "lib/files.bx";

function save(text: String) -> Int touches files {
    return file_write("out.txt", text);
}
```

Leave `touches files` off and it will not compile. The signature has to admit what the body reaches,
so you can read a call site and know whether it can write to your disk.

## …split a string

```burxt
use "lib/string.bx";

let parts: [String] = string_split("a,b,c", ",");
print(to_string(len(parts)) + ": " + parts[1]);
```

```
3: b
```

## …see everything a module offers

The [reference]({{ site.baseurl }}/reference/) has every keyword, builtin, command and
standard-library function, with a search box. It is generated by reading the compiler, so it cannot
describe a version that no longer exists.

## …find out why it refused

Read the message — they are written to tell you what to do, not just what went wrong.
[What it refuses]({{ site.baseurl }}/refused/) collects the important ones with the code that causes
each, and the fix beside it.

## …learn it properly rather than looking things up

[The guide]({{ site.baseurl }}/guide/) is thirteen chapters in reading order, starting from
installing it. Each one explains a decision and what it cost, because a rule is only worth learning
if you can see why it is there.

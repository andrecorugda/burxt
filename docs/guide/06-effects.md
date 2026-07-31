---
title: Effects
---

# 6. Effects — what a function can reach

## What this is for
{: #what-this-is-for}

You are reviewing a change. One function in a payments module gained four lines. The diff looks
fine. You approve it.

Six weeks later you find out those four lines call an audit endpoint over the network, on every
invoice, inside a database transaction. Nobody lied to you. The function's name did not change, its
parameters did not change, its return type did not change. **The only thing that changed was what
it could reach, and there was nowhere for that to show up.**

That is the gap this page is about.

## Think of a passport
{: #think-of-a-passport}

A function's signature is its passport. It says who it is (`invoice_total`), what it needs
(`lines: [Line]`), and what it gives back (`Decimal<2>`).

A passport also has stamps — where the holder has been. Burxt puts those in the signature too:

```burxt
function invoice_total(lines: [Line]) -> Decimal<2> { ... }
function audit(entry: String) -> Int touches network { ... }
function backup(path: String) -> Int touches files, commands { ... }
```

The first one has no stamps. It cannot read a file, run a program, ask the clock or speak to the
network — **not because it does not today, but because it may not**. If someone adds a line that
does, it stops compiling.

<figure>
<svg viewBox="0 0 680 268" role="img" aria-label="A signature as a passport: a function with no stamps may not reach the world at all, and a function that says touches files, network carries those two stamps and no others" style="max-width:100%;height:auto;">
  <style>
    .book  { fill: #ffffff; stroke: #1d1d1f; stroke-width: 2; }
    .page  { fill: #f5f5f7; stroke: #d2d2d7; stroke-width: 1; }
    .stamp { fill: none; stroke: #0f6f3c; stroke-width: 1.8; }
    .stampf{ fill: #0f6f3c; opacity: .08; }
    .blank { fill: none; stroke: #d2d2d7; stroke-width: 1.4; stroke-dasharray: 4 3; }
    .no    { fill: none; stroke: #c8102e; stroke-width: 2; }
    .hair  { fill: none; stroke: #d2d2d7; stroke-width: 1; }
    .h     { font: 600 13.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .t     { font: 11.5px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #1d1d1f; }
    .grn   { font: 600 11px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #0f6f3c; }
    .red   { font: 600 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #c8102e; }
    .cap   { font: 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
  </style>

  <text class="h" x="8" y="18">No stamps</text>

  <rect class="book" x="14" y="32" width="180" height="150" rx="8"/>
  <rect class="page" x="24" y="42" width="160" height="130" rx="4"/>
  <text class="t" x="32" y="62">invoice_total</text>
  <rect class="blank" x="34" y="76" width="60" height="38" rx="6"/>
  <rect class="blank" x="102" y="76" width="60" height="38" rx="6"/>
  <rect class="blank" x="34" y="122" width="60" height="38" rx="6"/>
  <rect class="blank" x="102" y="122" width="60" height="38" rx="6"/>

  <text class="red" x="14" y="204">It may not read a file, run a</text>
  <text class="red" x="14" y="221">program, ask the clock or speak</text>
  <text class="red" x="14" y="238">to the network. Not "does not" —</text>
  <text class="red" x="14" y="255"><tspan font-weight="700">may not</tspan>. A line that tries stops compiling.</text>

  <line class="hair" x1="336" y1="8" x2="336" y2="252"/>

  <text class="h" x="368" y="18">Two stamps, and only two</text>

  <rect class="book" x="374" y="32" width="180" height="150" rx="8"/>
  <rect class="page" x="384" y="42" width="160" height="130" rx="4"/>
  <text class="t" x="392" y="62">backup</text>
  <rect class="stampf" x="394" y="76" width="64" height="38" rx="6"/>
  <rect class="stamp"  x="394" y="76" width="64" height="38" rx="6"/>
  <text class="grn" x="404" y="100">files</text>
  <rect class="stampf" x="466" y="76" width="70" height="38" rx="6"/>
  <rect class="stamp"  x="466" y="76" width="70" height="38" rx="6"/>
  <text class="grn" x="472" y="100">commands</text>
  <rect class="blank" x="394" y="122" width="64" height="38" rx="6"/>
  <rect class="blank" x="466" y="122" width="70" height="38" rx="6"/>

  <text class="cap" x="374" y="204">It says so in the signature, so</text>
  <text class="cap" x="374" y="221">a reviewer learns it without</text>
  <text class="cap" x="374" y="238">opening the body — and a stamp</text>
  <text class="cap" x="374" y="255">travels to everyone who calls it.</text>
</svg>
<figcaption>A stamp is not a description of what the function does today. It is a limit on what it is
allowed to do, checked by the compiler and inherited by every caller.</figcaption>
</figure>

## A step closer
{: #a-step-closer}

### The six stamps

<div class="tablewrap" markdown="1">

| | |
|---|---|
| `files` | reads or writes the filesystem |
| `commands` | runs another program. The one that can do anything, so it stands alone |
| `clock` | the time, a random source — anything that answers differently for the same arguments |
| `input` | standard input, command-line arguments, the environment |
| `network` | speaks to something over a network |
| `model` | asks a language model |

</div>

The list is **closed**. That is not tidiness: if one library wrote `network` and another wrote
`net`, scanning a diff for "can this now reach the internet" would stop working, and scanning is
the entire point.

`print` is deliberately not on the list. It would be on almost every function, and an annotation
that appears on everything tells a reader nothing.

## In code
{: #in-code}

### It travels up the call chain

If you call something with a stamp, you need that stamp. All the way up.

<svg viewBox="0 0 620 190" role="img" aria-label="Effects travelling up a call chain" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .b { fill: #fff; stroke: #1d1d1f; stroke-width: 1.5; }
    .t { font: 13px ui-monospace, monospace; fill: #1d1d1f; }
    .s { font: 11px ui-monospace, monospace; fill: #c8102e; }
    .g { font: 11px ui-monospace, monospace; fill: #3a3a3e; }
    .a { stroke: #1d1d1f; stroke-width: 1.5; fill: none; marker-end: url(#h); }
  </style>
  <defs>
    <marker id="h" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/>
    </marker>
  </defs>
  <rect class="b" x="8" y="20" width="180" height="46" rx="4"/>
  <text class="t" x="20" y="40">charge_customer</text>
  <text class="s" x="20" y="57">touches network</text>

  <rect class="b" x="222" y="20" width="150" height="46" rx="4"/>
  <text class="t" x="234" y="40">send_receipt</text>
  <text class="s" x="234" y="57">touches network</text>

  <rect class="b" x="406" y="20" width="200" height="46" rx="4"/>
  <text class="t" x="418" y="40">http_post</text>
  <text class="s" x="418" y="57">touches network  ← declared</text>

  <path class="a" d="M190 43 L218 43"/>
  <path class="a" d="M374 43 L402 43"/>
  <text class="g" x="8" y="90">calls →</text>

  <rect class="b" x="8" y="120" width="180" height="46" rx="4"/>
  <text class="t" x="20" y="140">invoice_total</text>
  <text class="g" x="20" y="157">no stamps</text>

  <rect class="b" x="222" y="120" width="150" height="46" rx="4"/>
  <text class="t" x="234" y="140">line_subtotal</text>
  <text class="g" x="234" y="157">no stamps</text>

  <rect class="b" x="406" y="120" width="200" height="46" rx="4"/>
  <text class="t" x="418" y="140">price * quantity</text>
  <text class="g" x="418" y="157">arithmetic</text>

  <path class="a" d="M190 143 L218 143"/>
  <path class="a" d="M374 143 L402 143"/>
</svg>

The top row can reach the network and every signature says so. The bottom row cannot, and every
signature says that too — by staying silent.

Leave a stamp off and you get told, naming both ends:

```burxt
function send_receipt(to: String) -> Int {
    return http_post("https://api.example.com/receipt", to);
}
```

```
error: `http_post` touches network, but `send_receipt` does not say it does. Add
       `touches network` to `send_receipt`'s signature — so anyone reading it can see
       what this call can reach — or stop calling `http_post`.
```

### Where a stamp comes from in the first place

At the C boundary, and only there. `external function` has no body to reason about, so the
declaration is the only thing that can say what a C function reaches:

```burxt
external function system(command: String) -> CInt touches commands;
external function time(unused: Int) -> Int touches clock;
```

Everything above that is checked against it. An extern that declares nothing is taken at its word —
right for `strlen`, and a lie for `system`, which is why the standard library declares its own.

#### What that revealed about the standard library

Turning this on for the first time surfaced something nobody had noticed:

```burxt
external function system(command: String) -> CInt touches commands;

function file_exists(path: String) -> Bool touches commands {
    return system("test -e " + path) == 0;      // the real one quotes the path
}
```

**`file_exists` runs a subprocess.** It always did — it shells out to `test -e` rather than
stat-ing, which is defensible and which absolutely nobody reading `file_exists(path)` would guess.
`file_make_directory` and `file_list_directory` too, and `os_capture` writes a temporary file.

Four surprises in a nine-function module, on day one, from a feature whose whole job is surfacing
exactly that.

### Why it is written and not worked out

The compiler could infer this. It deliberately does not, and the reason is the difference between
this page and [page 4](04-memory.md), where `allocates` used to be written and is now inferred
away.

`allocates` was bookkeeping: it told a reader nothing they needed. `touches network` is a
**promise**. If the compiler inferred it, it would not be in the signature — and being in the
signature is the whole point, because it means:

> **Nothing can start reaching the network without a declaration changing.**

And a changed declaration is a thing a tool can find:

```
$ burxt review before.bx after.bx
WEAKENED  invoice_total   now touches network — it could not before
```

Which is the review you would otherwise have had to do by reading four lines of a diff at 5pm.

### `pure` is the same claim, from the other end

```burxt
pure function line_total(price: Decimal<2>, quantity: Int) -> Decimal<2> {
    return price * quantity;
}
```

`pure` means the answer depends on the arguments and nothing else — which *is* touching nothing,
plus a promise not to print. So saying both is a contradiction rather than a refinement:

```
error: `pure function f` cannot also `touches files`: `pure` means the answer depends on
       the arguments and nothing else, which is the same thing as touching nothing. Drop
       one of the two.
```

Use `pure` for a calculation. Use `touches` for a function that reaches the world and says so.

## Why it is built this way
{: #why-it-is-built-this-way}

**A written effect is a promise; an inferred one is only a fact.** The compiler could work out what a
function reaches — it does exactly that to check you. The reason it makes you write it anyway is that
inference tells you what the code does *today*, and a declaration says what it is *allowed* to do
tomorrow. Only the second one can be broken by a change, and only something that can be broken is worth
reviewing.

**It travels, so the boundary is real.** A stamp is inherited by every caller, transitively. That means
`touches network` on one leaf function surfaces on everything above it — which sounds like noise and is
the point: a total that can now reach the network is exactly the change you want to see in a diff.

**It is the opposite call from [`allocates`](04-memory.md), deliberately.** `allocates` became inferred
because it landed on three functions out of three and told a reader nothing. `touches network` is the
promise itself, so it stays written.

## What it costs
{: #what-it-costs}

**Adding one stamp can cascade up a call chain.** Turning this on in a nine-function module surfaced four
functions nobody had thought of as reaching the world. That is the feature working, and it is still a
morning's editing.

**Six effects, and no way to add a seventh.** `files`, `commands`, `clock`, `input`, `network`, `model` —
the set is closed. A domain-specific effect has nowhere to go.

**No granularity.** `touches files` says *files*, not *this path, read-only*. A function that reads one
config file carries the same stamp as one that deletes a directory.

### What is deliberately still missing

**Top-level code may reach anything.** There is no signature at a program's entry point for a
reviewer to read, because the file itself is what they are reading — and forbidding it would mean
no program could do I/O at all.

**Stage-1 parses `touches` and does not enforce it.** The Burxt-written compiler reads the word and
ignores it; stage-0 does the checking. Ignoring an effect declaration cannot miscompile anything,
because it changes no code.

**`model` has nothing to attach to yet.** Burxt has no model client, so the stamp exists for a
program that writes its own via `external function`. The rule it is there to enable —
*a function that produces money may not reach a model* — is written up in
[`spec/NOVELTY.md`](https://github.com/andrecorugda/burxt/blob/main/spec/NOVELTY.md) and not yet built. An LLM may decide what to do; it may
never decide what a number is.

## When you reach for it
{: #when-you-reach-for-it}

<div class="tablewrap" markdown="1">

| The function | Write |
|---|---|
| totals, formats, parses, calculates | nothing. No stamp is the strongest statement on this page |
| reads or writes a file | `touches files` |
| shells out, or asks whether a path exists | `touches commands` |
| reads the clock | `touches clock` |
| reads stdin or the command line | `touches input` |
| speaks to a socket | `touches network` |
| asks a model | `touches model` |
| depends only on its arguments, and you want that checked | `pure function` — the same claim from the other end |

</div>

The habit worth forming: **write the calculation without a stamp first.** If it will not compile, the
call that reaches the world is usually one you can move out — and moving it out is nearly always the
better program.

## Examples
{: #examples}

**A total with no stamps, and an audit call that has one.** `net_total` cannot reach anything, and the
compiler is what guarantees it:

```burxt
function net_total(lines: [Decimal<2>]) -> Decimal<2> {
    let mutable total: Decimal<2> = $0.00;
    for line in lines {
        total += line;
    }
    return total;
}

function audit(entry: String) -> Int touches files {
    return write_file("/dev/null", entry);
}

let lines: [Decimal<2>] = [$19.99, $36.80, $12.00];
print(net_total(lines));
print(audit("totalled"));
```

```
68.79
8
```

**And what happens when the total starts logging.** One line added inside `net_total`:

```burxt
function net_total(lines: [Decimal<2>]) -> Decimal<2> {
    let ignored: Int = write_file("/tmp/audit.log", "totalling");
    let mutable total: Decimal<2> = $0.00;
    for line in lines {
        total += line;
    }
    return total;
}
```

```
error: `write_file` touches files, but `net_total` does not say it does. Add `touches files` to `net_total`'s signature — so anyone reading it can see what this call can reach — or stop calling `write_file`.
 --> total.bx:2:24
  |
2 |     let ignored: Int = write_file("/tmp/audit.log", "totalling");
  |                        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Notice the two ways out the message offers, in the order it offers them: **say so**, or **stop**. It does
not suggest a flag, because there is not one.

## Next
{: #next}

[The C boundary](07-ffi.md) — where an effect enters the program, and where money stops being
exact in every other stack.

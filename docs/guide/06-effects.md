---
title: Effects
---

# 6. Effects — what a function can reach

## The problem, as it actually arrives

You are reviewing a change. One function in a payments module gained four lines. The diff looks
fine. You approve it.

Six weeks later you find out those four lines call an audit endpoint over the network, on every
invoice, inside a database transaction. Nobody lied to you. The function's name did not change, its
parameters did not change, its return type did not change. **The only thing that changed was what
it could reach, and there was nowhere for that to show up.**

That is the gap this page is about.

## Think of a passport

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

## The six stamps

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

## It travels up the call chain

If you call something with a stamp, you need that stamp. All the way up.

<svg viewBox="0 0 620 190" role="img" aria-label="Effects travelling up a call chain" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .b { fill: #fff; stroke: #111; stroke-width: 1.5; }
    .t { font: 13px ui-monospace, monospace; fill: #111; }
    .s { font: 11px ui-monospace, monospace; fill: #b00; }
    .g { font: 11px ui-monospace, monospace; fill: #888; }
    .a { stroke: #111; stroke-width: 1.5; fill: none; marker-end: url(#h); }
    @media (prefers-color-scheme: dark) {
      .b { fill: #1b1b1b; stroke: #ddd; } .t { fill: #eee; } .s { fill: #ff8080; }
      .a { stroke: #ddd; } .g { fill: #999; }
    }
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

## Where a stamp comes from in the first place

At the C boundary, and only there. `external function` has no body to reason about, so the
declaration is the only thing that can say what a C function reaches:

```burxt
external function system(command: String) -> CInt touches commands;
external function time(unused: Int) -> Int touches clock;
```

Everything above that is checked against it. An extern that declares nothing is taken at its word —
right for `strlen`, and a lie for `system`, which is why the standard library declares its own.

### What that revealed about the standard library

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

## Why it is written and not worked out

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

## `pure` is the same claim, from the other end

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

## What is deliberately still missing

**Top-level code may reach anything.** There is no signature at a program's entry point for a
reviewer to read, because the file itself is what they are reading — and forbidding it would mean
no program could do I/O at all.

**Stage-1 parses `touches` and does not enforce it.** The Burxt-written compiler reads the word and
ignores it; stage-0 does the checking. Ignoring an effect declaration cannot miscompile anything,
because it changes no code.

**`model` has nothing to attach to yet.** Burxt has no model client, so the stamp exists for a
program that writes its own via `external function`. The rule it is there to enable —
*a function that produces money may not reach a model* — is written up in
[`spec/NOVELTY.md`](../../spec/NOVELTY.md) and not yet built. An LLM may decide what to do; it may
never decide what a number is.

## Next

[Modules](07-modules.md) — `use`, one file per module, and what is visible.

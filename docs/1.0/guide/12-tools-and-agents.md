---
title: Tools and agents
description: The contract IS the tool schema, and a tool can tell you what a change did to what a program promises.
---

> **This documents Burxt 1.0.0, and it is frozen.** It will not change again — that is what makes
> it useful to someone still on 1.0. The current documentation is at
> [the top of the site]({{ site.baseurl }}/).


# 12. Tools and agents

## What this is for
{: #what-this-is-for}

An agent writes the code now. Two of this language's commands exist because of that, and they are
the two things in Burxt no other language can currently do.

**The first is about calling in.** An MCP tool ships a **JSON Schema** describing what a caller may
pass it. The function behind that tool also checks what it was passed. Those are *one fact written
twice*, and everywhere in the industry they drift:

```
schema says:    { "client_id": { "type": "integer", "minimum": 1 } }
handler says:   if client_id <= 0 { ... }        # someone remembered
handler says:   # ...or nobody did
```

The schema is the copy that rots. It keeps a bound the code relaxed a year ago, or says a field is
optional after the code started requiring it. The client sends a request that is valid *by the
schema*, the tool refuses it, and the failure arrives as a 500 rather than as a validation message —
which is the worst possible place to learn about it, because the schema was the thing meant to
prevent exactly that.

**The second is about changing out.** An agent could not satisfy a rule, so it deleted the rule. One
line, in a diff of forty. Every test still passes — more of them pass than before, because whatever
was failing was failing *on purpose*. In every other language an assertion in a body **is** just
another line in a body, and nothing in that diff looks different from any other deleted line.

That is the single most dangerous change anyone can make to a program.

## Think of a sign printed by the machine
{: #think-of-a-sign-printed-by-the-machine}

A parking meter has a sign saying which coins it takes, and a slot that actually takes them.

On most meters a person typed the sign. It was right the day it went up. Then the mechanism was
serviced, and now the sign says one thing and the slot does another — and you find out standing in
the rain with a coin that will not go in.

The other kind of meter prints its sign **from the mechanism**. Not carefully kept in step with it:
*printed from it*. There is no second thing to update, so there is nothing to forget.

<figure>
<svg viewBox="0 0 680 292" role="img" aria-label="Two parking meters: one with a hand-typed sign that has drifted from what the slot accepts, and one whose sign is printed from the slot itself so the two cannot disagree" style="max-width:100%;height:auto;">
  <style>
    .body  { fill: #ffffff; stroke: #1d1d1f; stroke-width: 2; }
    .glass { fill: #f5f5f7; stroke: #1d1d1f; stroke-width: 1.4; }
    .slot  { fill: #1d1d1f; }
    .post  { fill: none; stroke: #1d1d1f; stroke-width: 4; stroke-linecap: round; }
    .no    { fill: none; stroke: #c8102e; stroke-width: 2; }
    .drift { fill: none; stroke: #c8102e; stroke-width: 2; stroke-dasharray: 5 4; }
    .arrow { fill: none; stroke: #0f6f3c; stroke-width: 2; marker-end: url(#mg); }
    .hair  { fill: none; stroke: #d2d2d7; stroke-width: 1; }
    .h     { font: 600 13.5px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .t     { font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; fill: #1d1d1f; }
    .cap   { font: 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #1d1d1f; }
    .red   { font: 600 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #c8102e; }
    .grn   { font: 600 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #0f6f3c; }
    .hand  { font: italic 11px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #c8102e; }
  </style>
  <defs>
    <marker id="mg" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="#0f6f3c"/>
    </marker>
  </defs>

  <text class="h" x="8" y="18">A sign somebody typed</text>

  <rect class="body" x="20" y="34" width="136" height="164" rx="12"/>
  <rect class="glass" x="34" y="50" width="108" height="50" rx="6"/>
  <text class="t" x="42" y="70">accepts</text>
  <text class="t" x="42" y="88">£1 £2 £5</text>
  <text class="hand" x="34" y="116">typed once, by hand</text>
  <rect class="slot" x="62" y="136" width="52" height="9" rx="4"/>
  <text class="t" x="34" y="166">slot takes</text>
  <text class="t" x="122" y="166">£2</text>
  <path class="post" d="M88 198 v42"/>
  <path class="post" d="M68 240 h40"/>

  <path class="drift" d="M170 80 q24 40 0 78"/>
  <g class="no">
    <circle cx="182" cy="119" r="13"/>
    <line x1="173" y1="110" x2="191" y2="128"/>
  </g>
  <text class="red" x="204" y="106">the sign drifted;</text>
  <text class="red" x="204" y="123">the slot is the truth</text>
  <text class="cap" x="204" y="146">You find out in the rain.</text>

  <line class="hair" x1="392" y1="8" x2="392" y2="256"/>

  <text class="h" x="420" y="18">A sign the machine prints</text>

  <rect class="body" x="432" y="34" width="136" height="164" rx="12"/>
  <rect class="glass" x="446" y="50" width="108" height="36" rx="6"/>
  <text class="t" x="454" y="66">accepts</text>
  <text class="t" x="454" y="80">£2</text>
  <text class="grn" x="446" y="106">printed, not typed</text>
  <rect class="slot" x="474" y="136" width="52" height="9" rx="4"/>
  <text class="t" x="446" y="166">slot takes</text>
  <text class="t" x="534" y="166">£2</text>
  <path class="post" d="M500 198 v42"/>
  <path class="post" d="M480 240 h40"/>

  <path class="arrow" d="M582 150 q28 -46 -20 -74"/>
  <text class="grn" x="590" y="132">read</text>
  <text class="grn" x="590" y="149">from</text>
  <text class="grn" x="590" y="166">here</text>

  <line class="hair" x1="8" y1="256" x2="672" y2="256"/>
  <text class="cap" x="8" y="280">The slot is the precondition. The sign is the JSON Schema an agent validates against.</text>
</svg>
<figcaption><code>burxt mcp-schema</code> prints the second from the first, so forgetting to update it is
not a thing you can do — there is no second artifact to forget.</figcaption>
</figure>

## A step closer
{: #a-step-closer}

The reason no other language can do this is not tooling. It is that **the precondition lives in the
signature**.

```burxt
function line_total(unit: Decimal<2> [> $0.00], quantity: Int [> 0, <= 100000]) -> Decimal<2> {
    return unit * quantity;
}
```

`[> $0.00]` and `[> 0, <= 100000]` are contracts — the bracket spelling from
[contracts](05-contracts.md). They are checked at run time and they are *part of the declaration*, so
a tool that reads only declarations can see them.

In Python or TypeScript the same information is a decorator, or a separate schema object, or a
validation library's call inside the body. Keeping those in step with the checks is a code review —
which is precisely the review this language exists to remove.

And the clauses are read **structurally**, from the parsed condition, not from the text. So the
bracket form and a written `requires unit > $0.00` produce identical schema. That is what makes them
the same sentence rather than two spellings that happen to agree this week.

## In code
{: #in-code}

Ask the compiler for the manifest:

```sh
burxt mcp-schema examples/mcp/tools.bx
```

and the schema comes back derived from those clauses — `[> $0.00]` became `exclusiveMinimum`,
`[<= 100000]` became `maximum`:

```json
{"name":"line_total","inputSchema":{"type":"object","properties":{
  "unit":     {"type":"string","description":"Decimal<2>","exclusiveMinimum":"0.00"},
  "quantity": {"type":"integer","description":"Int","exclusiveMinimum":"0","maximum":"100000"}},
  "required":["unit","quantity"]}}
```

Money crosses as a **quoted string**, throughout. A JSON number reaches a JavaScript consumer as a
double and loses the cent; a string reaches every consumer intact. That is the same wall `as scaled`
puts at [the C boundary](07-ffi.md) and the same one `lib/json.bx` puts at the wire — three edges,
one idea.

### The other command: what a change did to what a program promises
{: #the-other-command}

`burxt review` compares two versions of a program and answers one question: **did any promise get
weaker?** Not what changed in the text — what changed in what the code guarantees.

```sh
burxt review before.bx after.bx
```

Given an `Account` that lost a precondition and had a `private` field opened up:

```
WEAKENED  Account.balance                    no longer `private` — anything may now read it
WEAKENED  Account.withdrawn                  lost `requires amount <= self.balance`

2 weakened promise(s). A weakened contract is the one change that passes every test — the tests were failing BECAUSE of it.
```

**It exits non-zero when a promise gets weaker**, so it works as a gate rather than as a report. Put
it in CI and nothing can quietly promise less than it did yesterday — not a deleted precondition, not
a function that started reaching the network, not a field that stopped being private.

## Why it is built this way
{: #why-it-is-built-this-way}

Both commands work for one reason, and it is the reason for every other decision in this language:
**everything that matters is in the signature.**

An agent reasons one function at a time. You scan. Neither of you has the whole program in view. So a
tool that reads only declarations can still answer the only question a review is really asking — and
that is only true because the declarations were made load-bearing on purpose.

That gives the schema a property a hand-written one cannot have. It is not *checked against* the
implementation, and it is not *generated from a comment*. It is derived from the same clause the
compiler enforces inside the body, so the agent calling the tool is bounded by the same rule the
function is, checked at both ends.

The doubled validation in `examples/mcp/server.bx` is deliberate for a related reason. The server
checks each argument and answers `-32602` before calling a tool; the tool *also* carries the
contract, which aborts the process if violated. A server must not die on a bad request, so the polite
check has to exist — and if the two ever disagreed, the contract would take the process down
**loudly** rather than letting a bad value through quietly.

## What it costs
{: #what-it-costs}

This is the honest part, and it is the most useful part of the page.

**A clause `mcp-schema` cannot express is skipped and reported — never guessed at.** JSON Schema has
no way to say that one parameter relates to another, so this:

```burxt
function withdraw(balance: Decimal<2>, amount: Decimal<2> [<= balance]) -> Decimal<2> {
    return balance - amount;
}
```

produces schema for `balance` and `amount` with **no bound on `amount` at all**, and says so on
stderr:

```
note: 1 precondition(s) could not be expressed as JSON Schema and were left out. A clause relating two parameters — `requires amount <= balance` — has no key in JSON Schema, and the function still enforces it.
```

Emitting something approximate there would be the drift this tool exists to remove. So the count goes
to stderr, and a schema that covers less than the function does says so out loud. **The function still
enforces it** — the caller simply learns about it as a refusal rather than as a validation message.

**The MCP server is an example, not a framework.** `examples/mcp/server.bx` handles `initialize`,
`tools/list` and `tools/call`. No `resources`, no `prompts`, no notifications, no batching. Enough to
be a real server and small enough to read in one sitting.

**`burxt review` reads promises, not behaviour.** It will tell you a precondition disappeared. It will
not tell you the body started computing the wrong total — that is what tests are for. The two are
complementary and neither replaces the other.

**Both are single-file today.** They read one program and its `use` graph, not a package.

## When you reach for it
{: #when-you-reach-for-it}

<div class="tablewrap" markdown="1">

| Situation | Reach for |
|---|---|
| exposing functions to an agent as MCP tools | `burxt mcp-schema`, and put the bounds in the signature so there is nothing else to write |
| a CI gate on what the code guarantees | `burxt review old.bx new.bx` — non-zero means a promise got weaker |
| reviewing an agent's diff by hand | read the declarations; that is where the changes that matter are |
| a tool that returns money | `Decimal<2>` in, a quoted string out — never a JSON number |
| a bound that relates two parameters | write it anyway. The function enforces it; the schema will tell you it could not carry it |

</div>

The one habit worth forming: **write the bound on the value, in the signature.** Everything on this
page is downstream of that, and it costs one bracket.

## Examples
{: #examples}

A real exchange with the server in `examples/mcp/`, recorded by building it and piping requests in.
Two of them succeed and two are refused, and the refusals are the interesting half.

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"line_total","arguments":{"unit":"19.99","quantity":3}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"tax_on","arguments":{"subtotal":"59.97","rate":"0.0825"}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"line_total","arguments":{"unit":"0.00","quantity":3}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"line_total","arguments":{"unit":"19.999","quantity":1}}}
```

```json
{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"59.97"}]}}
{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"4.95"}]}}
{"jsonrpc":"2.0","id":3,"error":{"code":-32602,"message":"`unit` must be greater than 0.00"}}
{"jsonrpc":"2.0","id":4,"error":{"code":-32602,"message":"`unit` must be an exact amount, as digits"}}
```

Four things to notice.

**`19.99 × 3` is `59.97`**, not `59.96999999999999`. The money went out and came back with all its
digits, as a string, because [that is what a `Decimal` is](02-numbers-and-money.md).

**`4.95` is a rounding that named itself.** `tax_on` returns
`Decimal<2, RoundHalfEven>`, so an agent reading the schema can see how the half-cent goes.

**Request 3 violated a precondition and got a JSON-RPC error** — and the process survived to answer
request 4. That is the doubled check doing its job.

**Request 4 sent `19.999` for a `Decimal<2>` and was refused, not rounded.** The caller sent a third
decimal place for a reason, and no default here can know what it was. This is the case that makes the
whole design worth it: rounding it silently would be the one behaviour that produces a plausible
wrong number.

Build it yourself:

```sh
burxt build examples/mcp/server.bx -o /tmp/mcp
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | /tmp/mcp
```

And there is nowhere for the schema to have drifted, because there is nowhere else for it to live.
`the_mcp_schema_follows_the_contracts` in `tests/runner.rs` proves that the only way it can:
it **tightens a clause and watches the schema move.** A test that compared the output to a recorded
string would pass forever while the derivation quietly stopped reading the clauses at all.

## Next
{: #next}

That is the guide. Two places to go from here.

The [reference]({{ site.baseurl }}/reference/) has every keyword, builtin, command and standard-library
function, with a search box — and it is generated by reading the compiler, so it cannot fall behind the
language.

The [examples]({{ site.baseurl }}/examples/) page has whole programs, including the point-of-sale till
written four times: once in Burxt, and once each in PHP, Python and Rust. Reading those side by side is
the shortest version of everything above.

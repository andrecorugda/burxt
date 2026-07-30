# An MCP server, in Burxt

```sh
burxt build examples/mcp/server.bx -o /tmp/mcp
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | /tmp/mcp
```

Two files: [`tools.bx`](tools.bx) is the money functions, [`server.bx`](server.bx) is JSON-RPC 2.0
over stdio, one message per line.

## Why it is here

Not to show that Burxt can speak a protocol. It is here for the one thing nothing else in this
repository can do:

```sh
$ burxt mcp-schema examples/mcp/tools.bx
```

reads this —

```burxt
function line_total(unit: Decimal<2> [> $0.00], quantity: Int [> 0, <= 100000]) -> Decimal<2>
```

— and answers this:

```json
{"name":"line_total","inputSchema":{"type":"object","properties":{
  "unit":     {"type":"string","description":"Decimal<2>","exclusiveMinimum":"0.00"},
  "quantity": {"type":"integer","description":"Int","exclusiveMinimum":"0","maximum":"100000"}},
  "required":["unit","quantity"]}}
```

**The schema is derived from the preconditions**, so it cannot drift from the implementation.

Everywhere else those are two artifacts maintained by hand, and the schema is the one that rots. It
says a field is optional after the code started requiring it, or keeps a bound the code relaxed a
year ago. The client sends a request that is valid by the schema, the tool refuses it, and the
failure arrives as a 500 rather than as a validation message — which is the worst place to learn
about it, because the schema was the thing meant to prevent exactly that.

Here there is one place to change. Forgetting to change the other is not possible, because there is
no other. `the_mcp_schema_follows_the_contracts` proves it by *tightening a clause and watching the
schema move* — a test that only compared the output to a recorded string would pass forever while
the derivation quietly stopped reading the clauses.

## Money crosses with all its digits

`19.99 × 3` answers `59.97`, from a string argument and a JSON-number argument alike. `19.999` asked
for as a `Decimal<2>` is **refused, never rounded** — the caller sent a third decimal place for a
reason and no default here can know what it was.

Money goes out as a **quoted string**. A JSON number reaches a JavaScript consumer as a double and
loses the cent; a string reaches every consumer intact. That is [`lib/json.bx`](../../lib/json.bx)'s
one position, and it is the same wall `as scaled` puts at the C boundary — three edges, one idea.

## Two things worth noticing in the code

**The validation is doubled on purpose.** The server checks each argument and answers `-32602` before
calling a tool; the tool *also* carries the contract, which aborts the process if violated. A server
must not die on a bad request, so the polite check has to exist — and if the two ever disagreed, the
contract would take the process down **loudly** rather than letting a bad value through quietly. The
redundancy is a tripwire on what would otherwise be a silent divergence, and both come from one
declaration.

**One region per request.** `region request { ... }` around the body of the loop is the whole of what
keeps a long-running server's memory flat: everything a request builds is released at the closing
brace. Without it the arena grows in a straight line and a busy server eventually reaches its 1 GB
reservation. See [Memory](../../docs/guide/04-memory.md).

## What it does not do yet

No `resources`, no `prompts`, no notifications, no batching. `initialize`, `tools/list` and
`tools/call` are enough to be a real server and small enough to read in one sitting, which is what an
example is for.

---
title: Types
---

# 3. Types

## The problem, as it actually arrives

Someone writes a careful class. The balance may not go negative, and they say so:

```burxt
class Account {
    owner: String,
    balance: Decimal<2>,

    function open(owner: String, opening: Decimal<2>) -> Account
        requires opening >= $0.00
    { return Account { owner: owner, balance: opening }; }
}
```

Three weeks later a test needs an overdrawn account. Nobody deletes the rule, nobody argues with
it — they simply do not go through the door:

```burxt
class Account { owner: String, balance: Decimal<2> }

let eve: Account = Account { owner: "eve", balance: -999.00 };
```

That compiles. And in that moment every `requires` in the class became a *suggestion*, because
there is a second way in that checks nothing. A reviewer looking at the diff sees one plausible
line in a test file.

**Privacy without a constructor is a locked door in an open wall.** This page is about building the
wall.

## Think of a sealed box with exactly one door

<svg viewBox="0 0 640 292" role="img" aria-label="A class with a private field: a literal and a field read bounce off the wall, the constructor is the only way in" style="max-width:100%;height:auto;margin:1.5rem 0;">
  <style>
    .b { fill: #fff; stroke: #111; stroke-width: 1.5; }
    .w { fill: none; stroke: #111; stroke-width: 2.5; }
    .p { fill: none; stroke: #b00; stroke-width: 1.5; stroke-dasharray: 5 4; }
    .door { fill: #fff; }
    .t { font: 12px ui-monospace, monospace; fill: #111; }
    .g { font: 11px ui-monospace, monospace; fill: #888; }
    .s { font: 11px ui-monospace, monospace; fill: #b00; }
    .a { stroke: #111; stroke-width: 1.5; fill: none; marker-end: url(#a3); }
    .x { stroke: #b00; stroke-width: 2; }
    @media (prefers-color-scheme: dark) {
      .b { fill: #1b1b1b; stroke: #ddd; } .w { stroke: #ddd; } .door { fill: #1b1b1b; }
      .t { fill: #eee; } .s { fill: #ff8080; } .p { stroke: #ff8080; }
      .a { stroke: #ddd; } .g { fill: #999; } .x { stroke: #ff8080; }
    }
  </style>
  <defs>
    <marker id="a3" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/>
    </marker>
  </defs>

  <rect class="b" x="8" y="30" width="212" height="46" rx="4"/>
  <text class="t" x="20" y="50">Account { owner: "eve",</text>
  <text class="t" x="20" y="66">          balance: -999.00 }</text>

  <rect class="b" x="8" y="126" width="212" height="46" rx="4"/>
  <text class="t" x="20" y="146">Account.open("ada",</text>
  <text class="t" x="20" y="162">             $100.00)</text>

  <rect class="b" x="8" y="222" width="212" height="30" rx="4"/>
  <text class="t" x="20" y="242">print(a.balance)</text>

  <rect class="w" x="348" y="22" width="284" height="256" rx="4"/>
  <rect class="door" x="342" y="140" width="13" height="38"/>

  <text class="g" x="362" y="44">class Account {</text>
  <rect class="b" x="362" y="54" width="252" height="26" rx="3"/>
  <text class="t" x="372" y="72">owner: String</text>
  <rect class="p" x="362" y="88" width="252" height="26" rx="3"/>
  <text class="t" x="372" y="106">private balance: Decimal&lt;2&gt;</text>

  <text class="t" x="362" y="152">function open(owner, opening)</text>
  <text class="s" x="362" y="168">    requires opening &gt;= $0.00</text>

  <text class="t" x="362" y="212">function (self) withdraw(amount)</text>
  <text class="s" x="362" y="228">    requires amount &lt;= self.balance</text>
  <text class="g" x="362" y="266">the only two ways in or out</text>

  <path class="a" d="M220 52 L330 58"/>
  <path class="x" d="M336 52 L350 66"/><path class="x" d="M350 52 L336 66"/>
  <text class="s" x="230" y="40">refused</text>

  <path class="a" d="M220 149 L344 156"/>
  <text class="g" x="240" y="134">allowed</text>

  <path class="a" d="M220 238 L330 232"/>
  <path class="x" d="M336 226 L350 240"/><path class="x" d="M350 226 L336 240"/>
  <text class="s" x="230" y="262">refused</text>
</svg>

Three pieces make that wall, and each is one word or one line.

## Piece one: fields and methods in the same block

```burxt
class Item {
    sku: String,
    name: String,
    price: Decimal<2>,

    function (self) label() -> String {
        return self.sku + " " + self.name;
    }
}

let rice: Item = Item { sku: "RICE", name: "Rice 5kg", price: 52.75 };
print(rice.label());        // RICE Rice 5kg
```

What a type *is* and what it *does*, findable in one jump. If you write PHP, C#, TypeScript or
Java, that is the shape you already expect.

A method may also be written outside the block — `function (self: Item) label() -> String { ... }`
— and that is what you need for a type someone else declared, in another file or in `lib/`. For
your own, the block is where they belong.

## Piece two: `private`

```burxt
class Account {
    owner: String,
    private balance: Decimal<2>,

    function (self) statement() -> String {
        return self.owner + " has " + to_string(self.spendable());
    }

    private function (self) spendable() -> Decimal<2> {
        return self.balance;
    }
}
```

From outside:

```burxt
class Account {
    owner: String,
    private balance: Decimal<2>,
    function open(owner: String) -> Account { return Account { owner: owner, balance: $0.00 }; }
}
let a: Account = Account.open("ada");
print(a.balance);
```

```
error: `Account.balance` is private: it is reachable only from `Account`'s own methods.
       Read it through a method that `Account` provides, or drop `private` from the field
       if it is part of the type's API.
```

**The class is the scope, not the file.** Another class's method cannot reach in and neither can
top-level code — but a method written *outside* the block on the same class still can, because the
boundary is the type.

There is no file boundary to appeal to, and that is not an oversight: `use` is a text pre-pass that
concatenates files ([page 8](08-modules.md)), so by the time anything is checked there are no files
left, only one long program. A class needs no such knowledge to be a boundary.

## Piece three: a constructor is a function with no `self`

```burxt
class Account {
    owner: String,
    private balance: Decimal<2>,

    // No `self`, because it MAKES one. Called as `Account.open(...)`.
    function open(owner: String, opening: Decimal<2>) -> Account
        requires opening >= $0.00
    {
        return Account { owner: owner, balance: opening };
    }
}

let a: Account = Account.open("ada", $100.00);
```

And now the literal from the top of this page has nowhere to go:

```burxt
class Account { owner: String, private balance: Decimal<2> }
let eve: Account = Account { owner: "eve", balance: -999.00 };
```

```
error: `Account.balance` is private, so `Account` cannot be built here: a literal may set a
       private field only inside `Account`. Give the class a constructor —
       `function open(...) -> Account` in its body, called as `Account.open(...)` — which is
       the point of making the field private.
```

A literal may name a private field **only inside its own class**. That single rule is what turns
the other two into a wall.

## Put together: a class that cannot lie about itself

```burxt
class Account {
    owner: String,
    private balance: Decimal<2>,

    function open(owner: String, opening: Decimal<2>) -> Account
        requires opening >= $0.00
    { return Account { owner: owner, balance: opening }; }

    function (self) withdraw(amount: Decimal<2>) -> Account
        requires amount > $0.00
        requires amount <= self.balance
    { return Account { owner: self.owner, balance: self.balance - amount }; }
}

let a: Account = Account.open("ada", $100.00);
let b: Account = a.withdraw($30.00);
```

`balance` cannot go negative. Not *should not* — **cannot**, and you can see why by reading the one
block: `open` is the only way to make one, `withdraw` is the only way to change it, and no literal
anywhere else can bypass either. Try it anyway and the contract names itself, with the clause you
wrote:

```
burxt runtime error: `requires amount <= self.balance` failed in `Account.withdraw`
```

There is no build mode that removes that check and no factory that skips it.
([Why this shipped after `private` rather than with it](../../spec/A5-CONTRACTS.md).)

## Values, not references

**Everything copies.** Assigning copies, passing to a function copies, storing in a field copies.
There is no hidden sharing to reason about and no aliasing bug to find:

```burxt
class Line { label: String, unit: Decimal<2>, quantity: Int }

let widget: Line = Line { label: "widget", unit: $19.99, quantity: 3 };
let mutable b: Line = widget;
b.quantity = 10;
print(widget.quantity);      // still 3
```

A literal must set **every** field. There is no default the compiler could invent that would be
right for money, and no half-built object to be caught holding.

`self` gets a copy; `mutable self` writes through to the caller's value:

```burxt
class Counter {
    n: Int,
    function (mutable self) bump() -> Int {
        self.n += 1;
        return self.n;
    }
}
```

Two spellings, two different promises, and the compiler holds you to whichever you wrote.

## Bindings, and where a type may be left out

```burxt
let count = 0;                       // Int
let price = $19.99;                  // Decimal<2>
let mutable running = 0;             // Int, and reassignable
```

A `let` takes its type from its value. **Nothing else infers.** Parameters, return types, fields
and every contract are written down, because a signature is what everyone who calls it reads — and
one you have to *compute* is not one you can read.

The one exception is an array, and it is not about the element type:

```burxt
let xs: [Int; 3] = [1, 2, 3];        // fixed: exactly three
let mutable lines: [String] = [];    // growable
```

`[1, 2, 3]` obviously holds `Int`s. What it does not say is **fixed or growable**, and those are
different types with different storage and different rules — so an array binding says which.

Inference removes typing, not checking:

```burxt
function wrong() -> Decimal<2> {
    let money = $1.00;               // Decimal<2>
    let rate = 8.25%;                // Decimal<4>
    return money + rate;
}
```

```
error: cannot + Decimal<2> and Decimal<4>: scales must match. Burxt does not
       silently rescale money.
```

And inference can never introduce rounding, because a rounding contract exists only if somebody
wrote one. An inferred binding has no annotation to read, so **hover in the editor is where its
type lives** — the honest trade is that the type did not disappear, it moved.

## Enums: one value, several shapes

```burxt
enum Status { Paid, Owing(Decimal<2>), Void }

function owed(s: Status) -> Decimal<2> {
    match s {
        Paid => { return $0.00; }
        Owing(amount) => { return amount; }
        Void => { return $0.00; }
    }
}
```

`match` is **exhaustive, and there is no wildcard**. Leave an arm out and it does not compile;
write the same arm twice and it does not compile.

The missing `_` is the point. Add a fourth variant a year from now and every `match` in the program
stops compiling until somebody says what it means there — which is the entire value of having sum
types at all. A wildcard trades that away for a few seconds today.

`match` works on `Int`, `String` and `Bool` too, so an ordinary switch does not have to be an
`if / else if` chain — which matters for a reviewer, because a chain of comparisons hides its shape
and a switch says *these are the cases* in one glance:

```burxt
function describe(status: Int) -> String {
    match status {
        200 => { return "ok"; }
        404 => { return "missing"; }
        500 => { return "broken"; }
        _   => { return "unknown"; }
    }
}
```

**Here `_` is required** — the exact opposite of the enum rule, and worth understanding rather than
memorising. An enum has a known, finite list, so listing all of it is the whole point and a
catch-all would throw away the error you want next year. `Int` cannot be enumerated, so a match
without a catch-all would be a hole with nothing to mark it. Same keyword, two rules, and each
message says which rule it is and why.

## Interfaces: the one abstraction mechanism

```burxt
interface Priced {
    function price(self) -> Decimal<2>
    function label(self) -> String
}

class Meal implements Priced {
    dish: String,
    cost: Decimal<2>,

    function (self) price() -> Decimal<2> { return self.cost; }
    function (self) label() -> String { return "meal: " + self.dish; }
}
```

`class Meal implements Priced` is the form to reach for: fields, methods and the promise in one
block. The standalone form is there for a class you cannot edit:

```burxt
class Book { title: String, cost: Decimal<2> }
interface Priced {
    function price(self) -> Decimal<2>
    function label(self) -> String
}

implement Priced for Book {
    function (self) price() -> Decimal<2> { return self.cost; }
    function (self) label() -> String { return "book: " + self.title; }
}
```

Conformance is **declared, not inferred**. A type does not accidentally satisfy an interface by
happening to have methods with the right names, and the compiler checks the implementation answers
every signature, adds none, and matches each one exactly — parameter count, parameter types, return
type and the receiver form.

Inside a class body or an `implement` block, `function (self) price()` needs no type: the header
already said which. Outside either, the method names its type, because there nothing else does.

### Using an interface as a type

```burxt
interface Priced { function price(self) -> Decimal<2> }
class Book { title: String, cost: Decimal<2> }
implement Priced for Book {
    function (self) price() -> Decimal<2> { return self.cost; }
}

function show(item: Priced) -> Decimal<2> { return item.price(); }
```

An interface name as a type means *any type that implements this*, decided at run time — a value
plus a table of the interface's methods. Static dispatch is the default everywhere else and costs
nothing; you pay for the indirection exactly where you asked for it. These are storable: bindings,
fields, arrays.

## Dependency injection is a field

No framework, no container, no annotation, no reflection:

```burxt
interface Rates {
    function rate_for(self, abroad: Bool) -> Decimal<4>
}

class TableRates implements Rates {
    home: Decimal<4>,
    abroad: Decimal<4>,
    function (self) rate_for(abroad: Bool) -> Decimal<4> {
        if abroad { return self.abroad; }
        return self.home;
    }
}

class FlatRates implements Rates {
    everywhere: Decimal<4>,
    function (self) rate_for(abroad: Bool) -> Decimal<4> { return self.everywhere; }
}

class Checkout {
    rates: Rates,                        // the dependency IS the field
    function (self) tax(subtotal: Decimal<2>, abroad: Bool) -> Decimal<2, RoundHalfEven> {
        return subtotal * self.rates.rate_for(abroad);
    }
}

let live_rates: TableRates = TableRates { home: 12.00%, abroad: 0.00% };
let live: Checkout = Checkout { rates: live_rates };
print(live.tax($100.00, false));          // 12.00

let stub_rates: FlatRates = FlatRates { everywhere: 5.00% };
let stubbed: Checkout = Checkout { rates: stub_rates };   // the seam a test uses
print(stubbed.tax($100.00, false));       // 5.00
```

`Checkout` never names either implementation and *cannot*. All it knows is that something answers
`Rates`.

One wrinkle to know before you meet it: the dependency must be **bound to a variable first**. An
interface object borrows the storage of the value it points at, and a temporary has none:

```burxt
interface Rates { function rate_for(self, abroad: Bool) -> Decimal<4> }
class FlatRates implements Rates {
    everywhere: Decimal<4>,
    function (self) rate_for(abroad: Bool) -> Decimal<4> { return self.everywhere; }
}
class Checkout { rates: Rates }
let c: Checkout = Checkout { rates: FlatRates { everywhere: 5.00% } };
```

```
error: a `dynamic Rates` must come from a variable — an interface object borrows the storage
       of the value it refers to, and an expression has none.
```

## Arrays

```burxt
let fixed: [Int; 3] = [1, 2, 3];      // length is part of the type
let mutable grow: [Int] = [];         // growable
let n: Int = push(grow, 42);
```

**Bounds are always checked.** An index the compiler can read is checked while compiling; the rest
at run time, and a failure names the index, the length and the position:

```
burxt runtime error: index 3 is outside an array of 3 (at byte 81)
```

Walking one:

```burxt
class Line { label: String, unit: Decimal<2>, quantity: Int }
let mutable lines: [Line] = [];
let n: Int = push(lines, Line { label: "widget", unit: $19.99, quantity: 3 });

for line in lines {
    print(line.label);
}
```

The element is a **copy**, immutable, and scoped to the body, so nothing can be written back into
the array through it. `break` and `continue` work as they read.

The thing iterated must be a **name or a field path** (`xs`, `self.items`), never a call — the loop
reads it once per element, so a call would pay its cost on every pass. Bind it first.

If you need the position, `while` is still there. There is no `for i in 0..n`: a range is a second
construct with its own questions, and this already says it.

```burxt
let mutable lines: [String] = [];
let n: Int = push(lines, "widget");
let mutable i = 0;
while i < len(lines) {
    print(lines[i]);
    i += 1;
}
```

## Strings

A `String` is bytes. `len` counts bytes, `byte_at` reads one, `substring` takes a slice, `+` joins.
Interpolation is a join written differently: `"total: {amount}"`. More in
[Maps and strings](11-maps.md).

## If you come from PHP, C# or Java

<div class="tablewrap" markdown="1">

| What you would reach for | Where it is |
|---|---|
| Constructor | `function open(...) -> Account` in the class, called `Account.open(...)` |
| Validation while constructing | `requires` on it — checked on **every** call, quoting the clause when it fails |
| Static / class method | The same thing: a function in the class with no `self` |
| Private field, private method | `private` |
| Interface | `interface`, and `class X implements Y` |
| Dependency injection | An interface-typed field. The caller chooses, with a literal instead of a container |
| Destructor / `IDisposable` | Nothing to write — see [Memory](04-memory.md) |
| `null` | Does not exist. Absence is a type: [`Option<T>`](10-absence-and-failure.md) |
| Inheritance | Deliberately absent — below |

</div>

[`examples/services.bx`](../../examples/services.bx) is that table as one running program.

## No inheritance, and why

No base type, no `extends`, no `super`, no abstract class. This was **decided and closed** in
v0.0.46, not deferred: across thirty versions nothing needed it, and composition plus interfaces
did the job every time — the same conclusion Go and Rust reached.

<div class="tablewrap" markdown="1">

| Instead of | Use |
|---|---|
| Shared fields | A field holding the common class |
| Shared behaviour | A function both types call |
| Shared *contract* | An interface, implemented by each |
| "Is-a" polymorphism | An interface used as a type |

</div>

What that buys is the absence of the fragile base class problem, of *which parent did this method
come from*, and of constructors running in an order nobody remembers.

## Next

[Memory](04-memory.md) — where built values live, and the one idea here with no equivalent
elsewhere.

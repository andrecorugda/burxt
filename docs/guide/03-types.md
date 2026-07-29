# 3. Types

## Bindings, and where the type goes

```burxt
let count = 0;                       // Int
let price = $19.99;                  // Decimal<2>
let origin = Point { x: 0, y: 0 };   // Point
let mutable running = 0;             // Int, and reassignable
```

A `let` takes its type from its value, so you write the annotation when it helps rather than
because the compiler insists. **Nothing else infers**: parameters, return types, record fields
and every contract are written down, because a signature is what everyone who calls a function
reads, and one you have to compute is not one you can read.

The one exception is an array, and it is not about the element type:

```burxt
let xs: [Int; 3] = [1, 2, 3];        // fixed: exactly three, no region needed
let mutable lines: [String] = [];    // growable: lives in a region
```

`[1, 2, 3]` obviously holds Ints. What it does not say is **fixed or growable**, and those are
different types with different storage and different rules. So an array binding says which.

Inference removes typing, not checking. Every rule still applies:

```burxt
let money = $1.00;                   // Decimal<2>
let rate = 8.25%;                    // Decimal<4>
let wrong = money + rate;            // error: scales must match
```

And it can never introduce rounding, because a rounding contract only exists if someone wrote
one — see [Numbers and money](02-numbers-and-money.md).

An inferred binding has no annotation to read, so **hover in the editor is where its type
lives**. That is the honest trade: the type did not disappear, it moved.

## Records

```burxt
record Line { label: String, unit: Decimal<2>, quantity: Int }

let widget: Line = Line { label: "widget", unit: $19.99, quantity: 3 };
```

**Value semantics, always.** Assigning copies; passing to a function copies; storing in a
record field copies. There is no hidden sharing to reason about:

```burxt
let mutable b: Line = widget;
b.quantity = 10;
print(widget.quantity);      // still 3
```

A record literal must set **every** field. There is no default value the compiler could
invent that would be right for money.

## Methods

Behaviour attaches to a type without a class:

```burxt
function (self: Line) amount() -> Decimal<2> { return self.unit * self.quantity; }
function (mutable self: Line) discount(by: Decimal<2>) -> Int {
    self.unit = self.unit - by;
    return 0;
}
```

`self` gets a copy; `mutable self` writes through to the caller's value. The two spellings are
different promises, and the compiler holds you to whichever you wrote.

## Enums

One value, several shapes:

```burxt
enum Status { Paid, Owing(Decimal<2>), Void }
```

`match` is **exhaustive and has no wildcard**:

```burxt
match s {
    Paid => { return $0.00; }
    Owing(amount) => { return amount; }
    Void => { return $0.00; }
}
```

Leaving an arm out is an error. So is writing the same arm twice. There is deliberately no
`_`: when you add a fourth variant, every match in the program stops compiling until you
say what it means there. That is the entire value of sum types — a wildcard throws it away
in exchange for a few seconds now.

## Traits — the abstraction mechanism

```burxt
trait Priced {
    function price(self) -> Decimal<2>
    function label(self) -> String
}

implement Priced for Book {
    function (self: Book) price() -> Decimal<2> { return self.cost; }
    function (self: Book) label() -> String allocates { return "book: " + self.title; }
}
```

Conformance is **declared, not inferred**: a type does not accidentally satisfy a trait by
having methods with the right names. The compiler checks the implementation answers every signature,
adds none, and matches each one exactly — parameter count, parameter types, return type,
and the receiver form.

### Inside an `implement`, the type is written once

```burxt
implement Priced for Book {
    function (self) price() -> Decimal<2> { return self.cost; }
    function (self) label() -> String allocates { return "book: " + self.title; }
}
```

The header said `Book`, so the methods need not repeat it. Outside an `implement` a method names
its type — `function (mutable self: Counter) bump() -> Int` — because there nothing else does.

## `dynamic` — runtime dispatch, only where you write it

```burxt
function show(item: dynamic Priced) -> Decimal<2> { return item.price(); }
```

Static dispatch is the default and costs nothing. `dynamic Priced` is a fat pointer — a value
plus a table of the trait's methods — and you pay for the indirection exactly where you
asked for it. `dynamic` values are storable: bindings, record fields, arrays.

## No inheritance, and why

There is no `class`, no base type, no `super`. This was **decided and closed**, not
deferred: across thirty versions nothing needed it, and composition plus traits did the
work every time.

What you would reach for a base class for:

| Instead of | Use |
|---|---|
| Shared fields | A record field holding the common record |
| Shared behaviour | A trait, implemented by each type |
| "Is-a" polymorphism | `dynamic Trait` |

What you avoid: the fragile base class problem, the question of which parent a method came
from, and constructors that run in an order nobody remembers.

## Arrays

```burxt
let fixed: [Int; 3] = [1, 2, 3];      // length is part of the type
let mutable grow: [Int] = [];         // growable, lives in a region
let n: Int = push(grow, 42);
```

**Bounds are always checked.** An index the compiler can read is checked at compile time; the
rest are checked at run time, and a failure names the index, the length and the position:

```
burxt runtime error: index 3 is outside an array of 3 (at byte 81)
```

## Walking an array

```burxt
for line in lines {
    print(line.label);
}
```

The element is a **copy**, immutable, and scoped to the body — so nothing can be written back
into the array through it. `break` and `continue` work as they read.

The array must be a **name or a field path** (`xs`, `self.items`), never a call: the loop reads
it once per element, so a call would pay its cost on every pass. Bind it first and iterate that.

If you need the position, `while` is still there:

```burxt
let mutable i = 0;
while i < len(lines) {
    print(i);
    print(lines[i].label);
    i += 1;
}
```

There is no `for i in 0..n`: a range is a second construct with its own questions, and the loop
above already says it.

## Strings

A `String` is bytes. `len` counts bytes, `byte_at` reads one, `substring` takes a slice, `+`
joins (and needs a region — see [Memory](04-memory.md)). Interpolation is a join written
differently: `"total: {amount}"`.

## Coming from classes

If your instinct is to reach for a class, here is where each thing it gives you lives.
[`examples/services.bx`](../../examples/services.bx) is all of it in one running program — a
checkout service with an injected tax-rate provider.

| What a class gives you | Burxt |
|---|---|
| **Constructor** | A function returning the record: `function new_order(...) -> Order` |
| **Validation while constructing** | `requires` on that function — checked on **every** call, quoting the clause when it fails, with no build mode that removes it and no factory that bypasses it |
| **Initialization** | A record literal must set **every field**. There is no half-built object and no `null` to leave in one |
| **Interface** | `trait` |
| **Reuse / polymorphism** | `implement Trait for Type`, and `dynamic Trait` where the choice is made at runtime |
| **Dependency injection** | A `dynamic Trait` **field**: `record Checkout { rates: dynamic Rates }`. The dependency is chosen by whoever builds the service — a record literal instead of a container |
| **Destructor / cleanup** | The [region](04-memory.md). You do not write one |
| **Static / class methods** | A free function. There is no class to hang them on |
| **Private fields** | Not yet — there is no `pub`, deferred with a trigger in [`spec/M6-MODULES.md`](../../spec/M6-MODULES.md) §1.2 |
| **Inheritance** | Deliberately absent |

### Dependency injection, concretely

```burxt
record Checkout { rates: dynamic Rates }          // the dependency is a field

let live_rates: TableRates = TableRates { home: 12.00%, abroad: 0.00% };
let live: Checkout = Checkout { rates: live_rates };

let stub_rates: FlatRates = FlatRates { everywhere: 5.00% };
let stubbed: Checkout = Checkout { rates: stub_rates };   // the seam a test uses
```

`Checkout` never names either implementation and cannot: all it knows is that something
answers `Rates`. No framework, no annotation, no container, no reflection.

One wrinkle worth knowing before you meet it: the dependency is **bound to a variable
first**. A `dynamic` borrows the storage of the value it refers to, and a temporary has none —
`Checkout { rates: FlatRates { ... } }` is refused, and the error says exactly that.

### Where this is stricter, not weaker

Three things in that example would have been allowed by a class-based language and are not
here: a record literal cannot omit a field, a precondition cannot be bypassed, and a trait
object cannot borrow a temporary. Each one is a bug class removed rather than a feature
withheld.

### And what is genuinely given up

**Inheritance.** No `extends`, no base class, no `super`, no abstract class. Shared behaviour
goes in a function both types call, or a record field both hold; shared *contract* goes in a
trait. That was closed in v0.0.46 after thirty versions in which nothing needed it — the same
conclusion Go and Rust reached — and what it buys is the absence of the fragile-base-class
problem, of "which parent did this method come from", and of constructors running in an order
nobody remembers.

## Next

[Memory](04-memory.md) — where built values live.

# What Burxt refuses

10 mistakes that **compile in every other language**, and what this compiler says instead.

Each one is code an agent writes confidently: it type-checks in Python, runs in PHP, and passes
review because nothing about it looks wrong. Read them and ask, honestly, which you would have
caught in a pull request at 5pm.

That is the whole argument. `examples/pos/` shows that the money is exact — this shows the part
that matters more: **every one of these is a review you no longer have to do.**

Two kinds of refusal appear below and the difference is not cosmetic. Most are caught at
**compile time**, before the program exists. One is a well-typed program that **stops** when a
value cannot be represented — calling that a compile error would misdescribe how the language
works.

Every message here was produced by running the program. `scripts/refused.py` regenerates this
file and a test diffs it, so the page cannot claim a refusal the compiler does not make.

---

## A rate added to a price

Both are decimals. One is money and one is a multiplier.

```burxt
// Adding a tax RATE to a money amount. Both are decimals, both look like numbers, and in
// Python, PHP, JavaScript or Java this computes something — just not the total.
let price: Decimal<2> = 19.99;
let rate:  Decimal<4> = 0.0825;
let total: Decimal<2> = price + rate;
print(total);
```

**Refused at compile time:**

```
error: cannot + Decimal<2> and Decimal<4>: scales must match. Burxt does not silently rescale money.
 --> /home/andre/burxt/examples/refused/01-mixed-scales.bx:5:25
  |
5 | let total: Decimal<2> = price + rate;
  |                         ^^^^^^^^^^^^
```

## Money times a rate, unrounded

The exact answer has six decimal places. Something has to decide which way two of them go.

```burxt
// Tax on a line: money times a rate. The exact answer has six decimal places, so landing it on
// two is a rounding — and somebody has to decide which way. Nothing here says.
let subtotal: Decimal<2> = 158.25;
let rate:     Decimal<4> = 0.1200;
let tax:      Decimal<2> = subtotal * rate;
print(tax);
```

**Refused at compile time:**

```
error: this multiplication of Decimal<2> by Decimal<4> has an exact product with 6 decimal places, and reaching Decimal<2> means rounding it. Say how — Decimal<2, RoundHalfEven> — or take the exact answer with Decimal<6>.
 --> /home/andre/burxt/examples/refused/02-unrounded-product.bx:5:28
  |
5 | let tax:      Decimal<2> = subtotal * rate;
  |                            ^^^^^^^^^^^^^^^
```

## A total past what an Int holds

Every other language wraps this to a negative and keeps going.

```burxt
// A running total that has grown past what an Int can hold. Every other language wraps this
// around to a negative number and keeps going.
let running: Int = 9223372036854775807;
print(running + 1);
```

**Stopped at run time:**

```
burxt runtime error: arithmetic overflow — the exact result no longer fits in the value range
```

## The constructor skipped

The class checks its invariant on the way in, so build one directly instead.

```burxt
// The class checks its own invariant on the way in. So build one directly instead — which is
// what an agent does when the constructor is inconvenient.
class Account {
    owner: String,
    private balance: Decimal<2>,

    function open(owner: String, opening: Decimal<2>) -> Account
        requires opening >= $0.00
    { return Account { owner: owner, balance: opening }; }
}
let a: Account = Account { owner: "eve", balance: $0.00 - $999.00 };
print(a.owner);
```

**Refused at compile time:**

```
error: `Account.balance` is private, so `Account` cannot be built here: a literal may set a private field only inside `Account`. Give the class a constructor — `function open(...) -> Account` in its body, called as `Account.open(...)` — which is the point of making the field private.
  --> /home/andre/burxt/examples/refused/04-bypassed-private.bx:11:18
   |
11 | let a: Account = Account { owner: "eve", balance: $0.00 - $999.00 };
   |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

## A case added after the code was written

A payment method joined the enum. This `match` predates it.

```burxt
// A payment method was added to the enum last week. This `match` was written before that and
// still compiles everywhere else, silently treating the new case as "none of the above".
enum Method { Cash, Card, Transfer }
function fee(m: Method) -> Decimal<2> {
    match m {
        Cash => { return $0.00; }
        Card => { return $0.30; }
    }
}
print(fee(Method.Transfer));
```

**Refused at compile time:**

```
error: this `match` on `Method` does not handle `Transfer`. Every variant must be handled — that is what makes adding a variant later a compile error instead of a silent fall-through.
 --> /home/andre/burxt/examples/refused/05-forgotten-variant.bx:7:19
  |
7 |         Card => { return $0.30; }
  |                   ^^^^^^^^^^^^^
```

## A precondition passed a value it forbids

The contract is in the signature, and the call still breaks it.

```burxt
// The contract is right there in the signature, and the call still breaks it. This is what an
// agent does with a value it did not check: it passes it.
function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>
    requires amount > $0.00
    requires amount <= balance
{
    return balance - amount;
}
print(withdraw($100.00, $30.00));
print(withdraw($100.00, $500.00));
```

**Stopped at run time:**

```
70.00
burxt runtime error: `requires amount <= balance` failed in `withdraw`
```

## Memory returned after it was freed

Built inside a `region` for tidiness, then handed to the caller.

```burxt
// A helper that builds a label inside a `region` so the memory is tidy, then returns it. The
// region releases at its closing brace, so the caller is handed freed bytes.
function label(id: Int) -> String {
    region scratch {
        let text: String = "item " + to_string(id);
        return text;
    }
}
print(label(7));
```

**Refused at compile time:**

```
error: cannot return this String: it was built inside a `region` block, which releases at its closing brace, so its storage would not outlive the call. Move the allocation out of the `region` block, or return a scalar summary.
 --> /home/andre/burxt/examples/refused/07-escaping-region.bx:6:9
  |
6 |         return text;
  |         ^^^^^^^^^^^^
```

## Text treated as money

A number that arrived from a model, a form or a CSV.

```burxt
// A number that arrived as text — from a model, a form, or a CSV. Every dynamic language lets you
// treat it as money and find out later.
let reply: String = "the total is 52.75";
let total: Decimal<2> = reply;
print(total);
```

**Refused at compile time:**

```
error: type mismatch in `let total`: declared Decimal<2>, but expression has type String
 --> /home/andre/burxt/examples/refused/08-text-as-money.bx:4:25
  |
4 | let total: Decimal<2> = reply;
  |                         ^^^^^
```

## An interface gained a method

The class satisfied it last week and still looks complete.

```burxt
// An interface gained a method. This class satisfied it last week and still looks complete.
interface Tax {
    function rate_for(self, taxable: Bool) -> Decimal<4>
    function label(self) -> String
}
class FlatTax implements Tax {
    rate: Decimal<4>,
    function (self) rate_for(taxable: Bool) -> Decimal<4> { return self.rate; }
}
let t: FlatTax = FlatTax { rate: 0.1200 };
print(t.rate_for(true));
```

**Refused at compile time:**

```
error: `class FlatTax implements Tax` is missing the method `label`. Every interface method must be implemented — Burxt has no default bodies.
 --> /home/andre/burxt/examples/refused/09-incomplete-interface.bx:6:1
  |
6 | class FlatTax implements Tax {
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

## A count compared with a price

Both are numbers to a human and to every dynamic language.

```burxt
// A quantity against a price. Both are numbers to a human and to every dynamic language; one is
// a count and the other is money.
let quantity: Int = 3;
let price: Decimal<2> = 3.00;
print(quantity == price);
```

**Refused at compile time:**

```
error: type error: cannot compare Int and Decimal<2> — the types must match exactly
 --> /home/andre/burxt/examples/refused/10-comparing-kinds.bx:5:7
  |
5 | print(quantity == price);
  |       ^^^^^^^^^^^^^^^^^
```

---

## What is not here

Nothing about performance, and nothing about syntax. Every refusal above is a **wrong answer
prevented** — a total that would have been short by a cent, a balance that would have gone
negative, a case that would have fallen through, freed memory handed back to a caller.

The list is also incomplete on purpose. `tests/fail/` holds over two hundred more, each with the
exact message it must produce, because a refusal that is not tested is a refusal that will
eventually stop happening.

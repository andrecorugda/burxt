// ============================================================================
// pos-rust/items.rs — what a shop sells, and what a customer is buying.
//
//     mod items;   // declared in till.rs, the crate root
//
// Data only. Every other file in this app depends on this one and it depends on
// nothing — the same layering as pos/items.bx.
// ============================================================================

/// Money, as whole cents.
///
/// Rust has no decimal type in its standard library. The real-world answer is a
/// crate — `rust_decimal` — and that is the fact worth noticing: exact money is
/// a DEPENDENCY DECISION in Rust and a default in Burxt. This file pays for it
/// by hand so the program needs nothing but rustc.
///
/// The newtype is the only thing stopping you adding a quantity to a price. It
/// exists because a bare `i64` would let you, and `Decimal<2>` would not.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Money(pub i64);

impl Money {
    pub const ZERO: Money = Money(0);

    /// Parse `"52.75"`. Written out because there is nothing to call.
    pub fn from_str(s: &str) -> Money {
        let (whole, frac) = s.split_once('.').expect("money needs a decimal point");
        Money(whole.parse::<i64>().unwrap() * 100 + frac.parse::<i64>().unwrap())
    }

    pub fn times(self, n: i64) -> Money {
        // `*` on i64 panics on overflow in debug and WRAPS in release, which is
        // the opposite of what money wants. Burxt traps in both.
        Money(self.0 * n)
    }

    pub fn plus(self, other: Money) -> Money {
        Money(self.0 + other.0)
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The trailing zero has to be forced: 3680 cents must print "36.80",
        // and `{}` on the remainder alone would give "36.8".
        write!(f, "{}.{:02}", self.0 / 100, (self.0 % 100).abs())
    }
}

/// One thing on the shelf.
///
/// Fields and behaviour in one block — `struct` plus its `impl` — which is the
/// difference worth noticing against items.bx: there, `line_subtotal` is a free
/// function beside `record Item` rather than something Item knows how to do.
#[derive(Clone)]
pub struct Item {
    pub sku: String,
    pub name: String,
    pub price: Money,
    pub taxable: bool,
}

/// One line on a receipt: a thing, and how many of it.
///
/// The Item is OWNED, not borrowed, because a Burxt record is a value and
/// `Line { item: Item }` copies it. Borrowing here would be the more idiomatic
/// Rust and would need a lifetime parameter on Line, on the basket, and on
/// every function that touches one — a whole vocabulary this program does not
/// otherwise need.
#[derive(Clone)]
pub struct Line {
    pub item: Item,
    pub quantity: i64,
}

impl Line {
    pub fn subtotal(&self) -> Money {
        self.item.price.times(self.quantity)
    }
}

/// The Burxt spelling, kept so the two programs read the same.
pub fn line_subtotal(line: &Line) -> Money {
    line.subtotal()
}

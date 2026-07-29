// ============================================================================
// pos-rust/till.rs — the app. Run this one:
//
//     rustc -O examples/pos-rust/till.rs -o /tmp/till && /tmp/till
//
// A static point-of-sale: a fixed catalogue, one sale, two tax rules, one
// receipt. Four files, and this is the only one that knows they exist.
//
// The three `mod` lines below are what `use "items.bx";` is. The difference:
// Burxt's `use` appears in every file that needs the module, and Rust's `mod`
// appears ONCE, here in the crate root — the other files reach each other with
// `use crate::items`. Declaring a module and importing from it are two separate
// ideas in Rust, and one idea in Burxt.
// ============================================================================

mod items;
mod receipt;
mod tax;

use items::{line_subtotal, Item, Line, Money};
use receipt::{line_text, totals_text};
use tax::{FlatTax, SplitTax, Tax};

// ---- The catalogue ---------------------------------------------------------
// A function rather than a constant, and here the reason is real: a `Vec` and a
// `String` are heap-allocated, and a `const` cannot allocate. catalogue() in
// till.bx is a function for the same shape of reason — a growable array needs a
// region to live in, and a top-level constant has none.
fn catalogue() -> Vec<Item> {
    vec![
        Item {
            sku: "RICE".to_string(),
            name: "Rice 5kg".to_string(),
            price: Money::from_str("52.75"),
            taxable: true,
        },
        Item {
            sku: "MILK".to_string(),
            name: "Milk 1L".to_string(),
            price: Money::from_str("18.40"),
            taxable: true,
        },
        Item {
            sku: "NEWS".to_string(),
            name: "Newspaper".to_string(),
            price: Money::from_str("12.00"),
            taxable: false,
        },
    ]
}

fn find_item(shelf: &[Item], sku: &str) -> Item {
    // `requires len(shelf) > 0` in till.bx is a checked precondition that names
    // itself when it fails. `assert!` is the closest thing, and it is a
    // statement in the body rather than part of the signature — so a caller
    // reading the declaration cannot see it.
    assert!(!shelf.is_empty(), "the catalogue is empty");
    for item in shelf {
        if item.sku == sku {
            return item.clone();
        }
    }
    shelf[0].clone()
}

// ---- Ringing up a sale -----------------------------------------------------
/// Two accumulators, because the receipt wants both and recomputing would be a
/// second pass over the same lines.
fn ring_up(rule: &dyn Tax, sale: &[Line]) {
    println!("--- {} tax ---", rule.label());
    let mut subtotal = Money::ZERO;
    let mut tax = Money::ZERO;
    for line in sale {
        println!("{}", line_text(line));
        subtotal = subtotal.plus(line_subtotal(line));
        tax = tax.plus(tax::line_tax(rule, line));
    }
    println!("{}", totals_text(subtotal, tax));
}

// ---- The program -----------------------------------------------------------
// Burxt has no entry point to declare: `region sale { ... }` at the top level IS
// the program. Rust needs `fn main`, and the name is load-bearing.
fn main() {
    let shelf = catalogue();
    let basket = vec![
        Line { item: find_item(&shelf, "RICE"), quantity: 3 },
        Line { item: find_item(&shelf, "MILK"), quantity: 2 },
        Line { item: find_item(&shelf, "NEWS"), quantity: 1 },
    ];

    // The same basket, priced two ways. Neither ring_up nor receipt.rs knows
    // which rule it is running — that is what the trait is for.
    let flat = FlatTax { rate: 1200 };
    let split = SplitTax { staples: 200, rest: 1200 };

    ring_up(&flat, &basket);

    println!();
    // The same lines again, under `split`: staples at 2%, everything else at 12%.
    ring_up(&split, &basket);
}

// ============================================================================
// pos-rust/receipt.rs — turning a sale into something a customer can read.
//
//     mod receipt;   // declared in till.rs, the crate root
//
// This is the file where the memory model becomes visible in BOTH languages,
// and the comparison is the interesting part of this directory.
//
// receipt.bx says `allocates` on every function: "I build in my caller's
// region", and release is one pointer reset for the whole sale. Here every
// String is its own heap allocation with its own free, tracked by ownership at
// compile time — no `allocates` needed, because the type system already knows
// who owns the result and when it drops.
//
// Two answers to the same question. Neither has a collector.
// ============================================================================

use crate::items::{line_subtotal, Line, Money};

/// A width-padded amount, so the column lines up.
///
/// `{:>10}` needs the value as a string first, because padding a Display impl
/// pads the OUTER format and Money's own `write!` ignores the width.
pub fn money_column(amount: Money) -> String {
    format!("{:>10}", amount.to_string())
}

pub fn line_text(line: &Line) -> String {
    format!(
        "{}  x{}{}",
        line.item.name,
        line.quantity,
        money_column(line_subtotal(line))
    )
}

/// The totals block. Three amounts, and the one that matters is the last.
///
/// Note what is NOT here: totals_text in receipt.bx has to take
/// `Decimal<2, RoundHalfUp>` rather than `Decimal<2>`, because a rounding
/// contract is part of the type and travels here from tax.bx even though this
/// file rounds nothing. Money carries no such record, so nothing in this
/// signature says the tax was rounded half-up and nothing checks it.
pub fn totals_text(subtotal: Money, tax: Money) -> String {
    let due = subtotal.plus(tax);
    format!(
        "subtotal{}\ntax     {}\ndue     {}",
        money_column(subtotal),
        money_column(tax),
        money_column(due)
    )
}

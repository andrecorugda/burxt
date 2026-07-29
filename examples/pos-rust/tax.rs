// ============================================================================
// pos-rust/tax.rs — how much tax a line attracts.
//
//     mod tax;   // declared in till.rs, the crate root
//
// A trait with two implementations, which is the shape any configurable service
// takes here too: the till does not know which rule it is running.
// ============================================================================

use crate::items::{line_subtotal, Item, Line, Money};

/// A rate, in ten-thousandths — 1200 is twelve percent.
///
/// This is where the absence of a decimal type hurts most. `Decimal<4>` says
/// "four decimal places" in the type; `i64` says nothing, so the unit lives in
/// this comment and in the name, and a caller who passes 12 instead of 1200
/// gets a hundredfold tax with no error anywhere.
pub type Rate = i64;

// ---- The interface ---------------------------------------------------------
pub trait Tax {
    fn rate_for(&self, item: &Item) -> Rate;
    fn label(&self) -> &'static str;
}

// ---- One rule: the same rate on everything taxable -------------------------
pub struct FlatTax {
    pub rate: Rate,
}

impl Tax for FlatTax {
    fn rate_for(&self, item: &Item) -> Rate {
        if !item.taxable {
            return 0;
        }
        self.rate
    }

    fn label(&self) -> &'static str {
        "flat"
    }
}

// ---- Another: staples cheaper than the rest --------------------------------
pub struct SplitTax {
    pub staples: Rate,
    pub rest: Rate,
}

impl Tax for SplitTax {
    fn rate_for(&self, item: &Item) -> Rate {
        if !item.taxable {
            return 0;
        }
        if item.sku == "RICE" || item.sku == "MILK" {
            return self.staples;
        }
        self.rest
    }

    fn label(&self) -> &'static str {
        "split"
    }
}

// ---- The tax on one line ---------------------------------------------------
/// `&dyn Tax` is Burxt's `dynamic Tax`: one machine function, the rule behind a
/// vtable, chosen at run time.
///
/// The rounding is the arithmetic below and nothing else. `+ 5_000` before
/// `/ 10_000` is half-up, it is only correct for positive amounts, and every
/// place in a program that multiplies money has to get it right. That is the
/// cost `Decimal<2, RoundHalfUp>` is paying for you — the rule is a name in the
/// return type, and the compiler emits this.
pub fn line_tax(rule: &dyn Tax, line: &Line) -> Money {
    let rate = rule.rate_for(&line.item);
    let base = line_subtotal(line);
    let exact = base.0 * rate; // cents × 1e-4: six decimal places, as an integer
    Money((exact + 5_000) / 10_000)
}

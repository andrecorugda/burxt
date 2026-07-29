# ============================================================================
# pos-python/tax.py — how much tax a line attracts.
#
#     from tax import Tax, FlatTax, SplitTax, line_tax
#
# An interface with two implementations, which is the shape any configurable
# service takes: the till does not know which rule it is running.
# ============================================================================

from decimal import Decimal, ROUND_HALF_UP
from typing import Protocol

from items import CENTS, Item, Line, line_subtotal

# A rate is `Decimal("0.1200")` — four decimal places, because a twelve-percent rate is not
# an amount of money. Python has no way to say that: this Decimal and a price are the same
# type, and adding them is legal. tax.bx refuses `$0.1200` by name.


class Tax(Protocol):
    """The interface.

    `Protocol` is structural, so a class does not declare that it implements
    this — it either has the methods or it fails at the call site, at run time.
    `implement Tax for FlatTax` is checked when the file is compiled.
    """

    def rate_for(self, item: Item) -> Decimal: ...
    def label(self) -> str: ...


# ---- One rule: the same rate on everything taxable -------------------------
class FlatTax:
    def __init__(self, rate: Decimal) -> None:
        self.rate = rate

    def rate_for(self, item: Item) -> Decimal:
        if not item.taxable:
            return Decimal("0.0000")
        return self.rate

    def label(self) -> str:
        return "flat"


# ---- Another: staples cheaper than the rest --------------------------------
class SplitTax:
    def __init__(self, staples: Decimal, rest: Decimal) -> None:
        self.staples = staples
        self.rest = rest

    def rate_for(self, item: Item) -> Decimal:
        if not item.taxable:
            return Decimal("0.0000")
        if item.sku in ("RICE", "MILK"):
            return self.staples
        return self.rest

    def label(self) -> str:
        return "split"


# ---- The tax on one line ---------------------------------------------------
def line_tax(rule: Tax, line: Line) -> Decimal:
    """The rate has four decimal places and the money has two, so the exact
    product has SIX, and landing it on two is a rounding.

    The rounding rule is written HERE, at the operation. Drop the `.quantize`
    and this silently returns a six-place number that then propagates into the
    total — no error, just a wrong receipt. In tax.bx the rule is part of the
    return type, so the compiler is the thing that remembers.
    """
    rate = rule.rate_for(line.item)
    base = line_subtotal(line)
    return (base * rate).quantize(CENTS, ROUND_HALF_UP)

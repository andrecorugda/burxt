# ============================================================================
# pos-python/items.py — what a shop sells, and what a customer is buying.
#
#     from items import Item, Line, line_subtotal
#
# Data only. Every other file in this app depends on this one and it depends on
# nothing — the same layering as pos/items.bx.
# ============================================================================

from dataclasses import dataclass
from decimal import Decimal

# Money is `decimal.Decimal`, and the exponent carries the scale. It is exact,
# but it is exact by CHOICE: `52.75` in this file would be a float and would be
# wrong by a fraction of a cent, silently. Nothing in the language stops you.
CENTS = Decimal("0.01")


@dataclass(frozen=True)
class Item:
    """One thing on the shelf.

    Fields and behaviour live in the same block, which is the difference worth
    noticing against items.bx: there, `line_subtotal` is a free function beside
    `record Item` rather than something Item knows how to do.
    """

    sku: str
    name: str
    price: Decimal
    taxable: bool


@dataclass(frozen=True)
class Line:
    """One line on a receipt: a thing, and how many of it."""

    item: Item
    quantity: int

    def subtotal(self) -> Decimal:
        # Decimal * int is exact and keeps the scale, so no rounding happens
        # here. If `price` were a float this would be the first place it drifted.
        return self.item.price * self.quantity


def line_subtotal(line: Line) -> Decimal:
    """The Burxt spelling, kept so the two programs read the same."""
    return line.subtotal()

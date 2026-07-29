# ============================================================================
# pos-python/till.py — the app. Run this one:
#
#     python3 examples/pos-python/till.py
#
# A static point-of-sale: a fixed catalogue, one sale, two tax rules, one
# receipt. Four files, and this is the only one that knows they exist.
# ============================================================================

from decimal import Decimal

from items import Item, Line, line_subtotal
from receipt import line_text, totals_text
from tax import FlatTax, SplitTax, Tax, line_tax


# ---- The catalogue ---------------------------------------------------------
# A function rather than a module constant, which here is only a habit — there
# is no region to be outside of. catalogue() in till.bx is a function because a
# growable array needs a region to live in, and a top-level constant has none.
def catalogue() -> list[Item]:
    return [
        Item("RICE", "Rice 5kg", Decimal("52.75"), True),
        Item("MILK", "Milk 1L", Decimal("18.40"), True),
        Item("NEWS", "Newspaper", Decimal("12.00"), False),
    ]


def find_item(shelf: list[Item], sku: str) -> Item:
    # `requires len(shelf) > 0` in till.bx is a checked precondition. The
    # closest honest equivalent is an assert, which runs unless someone passes
    # -O, and which nobody writes.
    assert shelf, "the catalogue is empty"
    for item in shelf:
        if item.sku == sku:
            return item
    return shelf[0]


# ---- Ringing up a sale -----------------------------------------------------
def ring_up(rule: Tax, sale: list[Line]) -> None:
    """Two accumulators, because the receipt wants both and recomputing would
    be a second pass over the same lines."""
    print(f"--- {rule.label()} tax ---")
    subtotal = Decimal("0.00")
    tax = Decimal("0.00")
    for line in sale:
        print(line_text(line))
        subtotal += line_subtotal(line)
        tax += line_tax(rule, line)
    print(totals_text(subtotal, tax))


# ---- The program -----------------------------------------------------------
def main() -> None:
    shelf = catalogue()
    basket = [
        Line(find_item(shelf, "RICE"), 3),
        Line(find_item(shelf, "MILK"), 2),
        Line(find_item(shelf, "NEWS"), 1),
    ]

    # The same basket, priced two ways. Neither ring_up nor receipt.py knows
    # which rule it is running — that is what the interface is for.
    flat = FlatTax(Decimal("0.1200"))
    split = SplitTax(Decimal("0.0200"), Decimal("0.1200"))

    ring_up(flat, basket)

    print("")
    # The same lines again, under `split`: staples at 2%, everything else at 12%.
    ring_up(split, basket)


# Burxt has no entry point to declare: `region sale { ... }` at the top level IS
# the program. Python needs this guard, because importing a module runs it.
if __name__ == "__main__":
    main()

# ============================================================================
# pos-python/receipt.py — turning a sale into something a customer can read.
#
#     from receipt import line_text, totals_text
#
# Compare receipt.bx, which is where the memory model becomes visible: every
# function there says `allocates`, because a built String needs somewhere to
# live. Here every string is heap-allocated and collected whenever the GC feels
# like it, and there is nothing in the signature to say so.
# ============================================================================

from decimal import Decimal

from items import Line, line_subtotal


def money_column(amount: Decimal) -> str:
    """A width-padded amount, so the column lines up.

    `str(Decimal("36.80"))` keeps the trailing zero, because a Decimal carries
    its exponent. `str(36.80)` — a float — would give "36.8" and break the
    column.
    """
    return f"{amount:>10}"


def line_text(line: Line) -> str:
    return f"{line.item.name}  x{line.quantity}{money_column(line_subtotal(line))}"


def totals_text(subtotal: Decimal, tax: Decimal) -> str:
    """The totals block. Three amounts, and the one that matters is the last.

    Note what is NOT here: totals_text in receipt.bx has to take
    `Decimal<2, RoundHalfUp>` rather than `Decimal<2>`, because a rounding
    contract is part of the type and travels here from tax.bx even though this
    file rounds nothing. Python has no such cost — and no such record. Nothing
    in this signature says the tax was rounded half-up, so nothing checks it.
    """
    due = subtotal + tax
    return (
        f"subtotal{money_column(subtotal)}\n"
        f"tax     {money_column(tax)}\n"
        f"due     {money_column(due)}"
    )

<?php
// ============================================================================
// pos-php/items.php — what a shop sells, and what a customer is buying.
//
//     require_once __DIR__ . '/items.php';
//
// Data only. Every other file in this app depends on this one and it depends on
// nothing — the same layering as pos/items.bx.
// ============================================================================

declare(strict_types=1);

// Money is a bcmath STRING, at scale 2. PHP's own numbers are floats, where
// `0.1 + 0.2 !== 0.3`, so an amount that stays correct cannot be a number at
// all. `'52.75'` looks like text because to PHP it IS text.
const SCALE = 2;

/**
 * Half-up at 2 dp.
 *
 * bcmath TRUNCATES, so this adds half of the last place before letting bcadd
 * cut it. Correct for positive amounts only — a refund needs the sign handled,
 * and forgetting that is the classic PHP money bug. In tax.bx `RoundHalfUp` is
 * a name in the type and the compiler emits the rounding.
 */
function round_half_up(string $value): string
{
    return bcadd($value, '0.005', SCALE);
}

/**
 * One thing on the shelf.
 *
 * Fields and behaviour in one block, which is the difference worth noticing
 * against items.bx: there, `line_subtotal` is a free function beside
 * `record Item` rather than something Item knows how to do.
 */
final class Item
{
    public function __construct(
        public readonly string $sku,
        public readonly string $name,
        public readonly string $price,     // a decimal string, not a float
        public readonly bool $taxable,
    ) {
    }
}

/** One line on a receipt: a thing, and how many of it. */
final class Line
{
    public function __construct(
        public readonly Item $item,
        public readonly int $quantity,
    ) {
    }

    public function subtotal(): string
    {
        // Not `$price * $quantity`. That would coerce the string to a float and
        // the receipt would be wrong in the fourth decimal place, invisibly.
        return bcmul($this->item->price, (string) $this->quantity, SCALE);
    }
}

/** The Burxt spelling, kept so the two programs read the same. */
function line_subtotal(Line $line): string
{
    return $line->subtotal();
}

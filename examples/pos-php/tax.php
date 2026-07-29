<?php
// ============================================================================
// pos-php/tax.php — how much tax a line attracts.
//
//     require_once __DIR__ . '/tax.php';
//
// An interface with two implementations, which is the shape any configurable
// service takes: the till does not know which rule it is running.
// ============================================================================

declare(strict_types=1);

require_once __DIR__ . '/items.php';

// A rate is `'0.1200'` — four decimal places, because a twelve-percent rate is not an
// amount of money. PHP cannot say that: a rate and a price are both `string`, and the
// type declaration on every function below says `string` too. tax.bx refuses `$0.1200`
// by name, because `$` means Decimal<2>.

// ---- The interface ---------------------------------------------------------
interface Tax
{
    public function rateFor(Item $item): string;

    public function label(): string;
}

// ---- One rule: the same rate on everything taxable -------------------------
final class FlatTax implements Tax
{
    public function __construct(private readonly string $rate)
    {
    }

    public function rateFor(Item $item): string
    {
        return $item->taxable ? $this->rate : '0.0000';
    }

    public function label(): string
    {
        return 'flat';
    }
}

// ---- Another: staples cheaper than the rest --------------------------------
final class SplitTax implements Tax
{
    public function __construct(
        private readonly string $staples,
        private readonly string $rest,
    ) {
    }

    public function rateFor(Item $item): string
    {
        if (!$item->taxable) {
            return '0.0000';
        }
        return in_array($item->sku, ['RICE', 'MILK'], true) ? $this->staples : $this->rest;
    }

    public function label(): string
    {
        return 'split';
    }
}

// ---- The tax on one line ---------------------------------------------------
/**
 * The rate has four decimal places and the money has two, so the exact product
 * has SIX, and landing it on two is a rounding.
 *
 * Both steps are visible and both are on you: scale 6 first so nothing is lost
 * before the rounding decides, then the half-up. Write `bcmul(..., 2)` instead
 * and the product is truncated before rounding — 4.416 becomes 4.41 instead of
 * 4.42, and the receipt is a cent light with no error anywhere.
 */
function line_tax(Tax $rule, Line $line): string
{
    $rate = $rule->rateFor($line->item);
    $base = line_subtotal($line);
    return round_half_up(bcmul($base, $rate, 6));
}

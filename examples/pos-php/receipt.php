<?php
// ============================================================================
// pos-php/receipt.php — turning a sale into something a customer can read.
//
//     require_once __DIR__ . '/receipt.php';
//
// Compare receipt.bx, which is where the memory model becomes visible: every
// function there says `allocates`, because a built String needs somewhere to
// live. Here strings are refcounted and freed whenever the count hits zero, and
// there is nothing in the signature to say so.
// ============================================================================

declare(strict_types=1);

require_once __DIR__ . '/items.php';
require_once __DIR__ . '/tax.php';

/**
 * A width-padded amount, so the column lines up.
 *
 * The amount is already a scale-2 string, which is the one advantage of money
 * as text: '36.80' keeps its trailing zero for free. A float would print '36.8'
 * and break the column.
 */
function money_column(string $amount): string
{
    return str_pad($amount, 10, ' ', STR_PAD_LEFT);
}

function line_text(Line $line): string
{
    return $line->item->name . '  x' . $line->quantity . money_column(line_subtotal($line));
}

/**
 * The totals block. Three amounts, and the one that matters is the last.
 *
 * Note what is NOT here: totals_text in receipt.bx has to take
 * `Decimal<2, RoundHalfUp>` rather than `Decimal<2>`, because a rounding
 * contract is part of the type and travels here from tax.bx even though this
 * file rounds nothing. PHP has no such cost — and no such record. Both
 * parameters are `string`, so this function cannot tell money from a name.
 */
function totals_text(string $subtotal, string $tax): string
{
    $due = bcadd($subtotal, $tax, SCALE);
    return 'subtotal' . money_column($subtotal) . "\n"
        . 'tax     ' . money_column($tax) . "\n"
        . 'due     ' . money_column($due);
}

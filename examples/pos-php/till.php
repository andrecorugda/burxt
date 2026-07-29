<?php
// ============================================================================
// pos-php/till.php — the app. Run this one:
//
//     php examples/pos-php/till.php
//
// A static point-of-sale: a fixed catalogue, one sale, two tax rules, one
// receipt. Four files, and this is the only one that knows they exist.
// ============================================================================

declare(strict_types=1);

require_once __DIR__ . '/items.php';
require_once __DIR__ . '/tax.php';
require_once __DIR__ . '/receipt.php';

// ---- The catalogue ---------------------------------------------------------
// A function rather than a constant, which here is only a habit — there is no
// region to be outside of. catalogue() in till.bx is a function because a
// growable array needs a region to live in, and a top-level constant has none.
/** @return Item[] */
function catalogue(): array
{
    return [
        new Item('RICE', 'Rice 5kg', '52.75', true),
        new Item('MILK', 'Milk 1L', '18.40', true),
        new Item('NEWS', 'Newspaper', '12.00', false),
    ];
}

/** @param Item[] $shelf */
function find_item(array $shelf, string $sku): Item
{
    // `requires len(shelf) > 0` in till.bx is a checked precondition that names
    // itself when it fails. This is the closest honest equivalent, and it is a
    // library call rather than part of the signature.
    assert(count($shelf) > 0, 'the catalogue is empty');
    foreach ($shelf as $item) {
        if ($item->sku === $sku) {
            return $item;
        }
    }
    return $shelf[0];
}

// ---- Ringing up a sale -----------------------------------------------------
/**
 * Two accumulators, because the receipt wants both and recomputing would be a
 * second pass over the same lines.
 *
 * @param Line[] $sale
 */
function ring_up(Tax $rule, array $sale): void
{
    echo '--- ', $rule->label(), " tax ---\n";
    $subtotal = '0.00';
    $tax = '0.00';
    foreach ($sale as $line) {
        echo line_text($line), "\n";
        // `+=` would be a float addition. Every accumulation has to be a call.
        $subtotal = bcadd($subtotal, line_subtotal($line), SCALE);
        $tax = bcadd($tax, line_tax($rule, $line), SCALE);
    }
    echo totals_text($subtotal, $tax), "\n";
}

// ---- The program -----------------------------------------------------------
// No guard needed: this file is the entry point and nothing requires it. Closer
// to Burxt's `region sale { ... }` than Python's __main__ dance — though PHP
// will happily run this file as a web request too.
$shelf = catalogue();
$basket = [
    new Line(find_item($shelf, 'RICE'), 3),
    new Line(find_item($shelf, 'MILK'), 2),
    new Line(find_item($shelf, 'NEWS'), 1),
];

// The same basket, priced two ways. Neither ring_up nor receipt.php knows which
// rule it is running — that is what the interface is for.
$flat = new FlatTax('0.1200');
$split = new SplitTax('0.0200', '0.1200');

ring_up($flat, $basket);

echo "\n";
// The same lines again, under `split`: staples at 2%, everything else at 12%.
ring_up($split, $basket);

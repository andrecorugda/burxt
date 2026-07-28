/* The C half of examples/ffi.bx. Build and run with:
 *
 *     cc -c examples/ffi.c -o /tmp/ffi.o
 *     burxt run examples/ffi.bx /tmp/ffi.o -o /tmp/ffi
 */
#include <stdio.h>

/* Burxt hands a Decimal<2> across as its SCALED INTEGER — $19.99 arrives as
 * 1999, not as 19.99 — so the exactness survives a boundary where a `double`
 * would have destroyed it. The scale is part of the contract, and both sides
 * agree on it because the Burxt declaration says `as scaled`. */
long long cents_doubled(long long scaled_amount) {
    return scaled_amount * 2;
}

int shout(const char *text) {
    return printf("C says: %s\n", text);
}

# The BMX conformance suite, vendored

**This directory is a copy. The format lives in its own repository, and that copy is the
authority.** It is vendored here for one reason: Burxt's test suite must be able to prove its
BMX implementation conforms without a network, and a suite fetched at test time is a suite that
can go red because someone else pushed.

`harness.py` is copied unchanged and deliberately not rewritten in Rust. **It is the standalone
claim, tested rather than asserted**: the cases are data, so a conformance run is thirty lines of
a language that is not the implementation's. Rewriting it in the test harness's own language
would quietly delete the thing it demonstrates.

## Updating

Copy the directory again when the format's version changes, and say which version in the commit
message. The suite IS the format's semver — a case that had to be *edited* rather than *added*
is a major, per its `VERSIONING.md` — so a diff here that touches an existing file is a fact
worth reading before it is merged.

Vendored at **BMX 0.2**.

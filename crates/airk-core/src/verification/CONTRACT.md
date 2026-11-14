# Saved-source verification projection validation

Core owns receipt and projection semantics. Source reads and receipt production
remain in the existing Kernel and native member owners. No wire or digest
format changes here.

A final combined-verification proof must identify the exact outer native read
authority. Parse projections retain their existing root scope_identity. An exact
airk.combined-read/1 root instead uses its typed, validated PublicationVector's
authority; it has no duplicate root scope field. Its actual ordered member
roster must equal the publication vector roster, and every proof member must
belong to that roster. Missing/invalid vectors, foreign authorities or outside
members refuse. Equal source text never supplies missing index authority.

The existing full reference checks remain mandatory: materialize native member
codecs, map the longest explicit member pointer binding, compare each checked
reference byte-for-byte at its final returned pointer, require the nearest
explicit index to match, and account for the complete final reference inventory.
No removed reference, altered citation, clipped evidence or cross-member rebind
can retain its source claim. Validation never mutates the returned graph.

Focused cases use the complete Rust-emitted two-member Answer fixture, then
attach independently specified receipt locators. These pure cases prove
projection semantics, not filesystem verification or transport qualification.

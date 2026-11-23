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

**Receipt allocation** (change-log entry 53). A receipt takes
`(frame - quoted reply - 2048) / 2` bytes, at most 32,768, where the quoted
reply is the reply as escaped JSON text (`quoted_len`) and `frame` is the
calling tool's own reply ceiling: 131,072 for Answer /3, 65,536 for every
other tool (`receipt_budget_within`; `receipt_budget_for_reply` is the
65,536 case). Below 1,024 bytes the call is refused with
`verification_reply_budget` and no source check runs, the one rule for every
tool. An Answer never reaches that refusal: its Union packs the escaped
reply into the frame less a reserve of twice the receipt floor plus 4,096
(the 2,048 above, and 2,048 for the annotations the handler adds after
Union, as the 4,096 reserve without verification), so the receipt always
gets at least its floor. Within the allocation the kernel keeps source rows
in order and discloses the rest in the receipt (`source_rows_omitted`,
`all_references_accounted: false`, `limits.hit` includes `receipt_bytes`).
Before, an Answer's receipt was measured against the 65,536 frame and the
Union packed the plain encoding, so an Answer reply above about 61 KiB
failed as a whole.

Focused cases use the complete Rust-emitted two-member Answer fixture, then
attach independently specified receipt locators. These pure cases prove
projection semantics, not filesystem verification or transport qualification.

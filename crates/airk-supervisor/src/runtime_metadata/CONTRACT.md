# Running process build observations

Owner: Supervisor runtime_metadata; Core owns the unchanged RuntimeBuild wire.
Host RuntimePair and Core SplitRuntimeMetadata own native pair identity.
C7 build receipts remain owned by scripts/split_receipts.py and split/contracts.

RuntimeBuild is a bounded path-free observation of the actual process source,
not executable, installation, signing, publication or qualification authority.
Missing or invalid evidence stays build=None with the existing metadata error.

Historical schema1 receipt projection remains unchanged: the actual image hash
must match the native-version inventory; the complete build receipt bytes must
match that inventory; inventory limit512KiB, receipt16KiB, profile32bytes and
rustc2048bytes remain. Unknown receipt schemas refuse.

Additive C7schema2 observation requires the recognized existing C7 top-level
shape, exact Source keys/files1..1_000_000 and original profile/rustc, full Core SplitRuntimeMetadata, exact
current version/OS/architecture and a declared debug/release/release-local
profile. Current C7 rustc must be nonempty and at most4096 Unicode characters,
with C0 controls refused except CR/LF/TAB, matching the existing writer.
All five report blocks are required JSON objects; their contents are opaque
sealed reports, never model/build/release qualification. The original receipt's full pair declaration must equal the actual
native Host-validated schema3 pair metadata. Both pair metadata and CLI digest
must be inventory-bound; actual CLI must be an ordinary unlinked file and its
hash must equal that declaration. The actual running main hash is already
observed by the trusted process caller/cache and must equal the same pair;
this function does not independently rehash main or every other payload file.
Whole-inventory binding means the SHA of the complete original manifest bytes,
not installer-style full-payload qualification. Duplicate or unknown/missing C7 top-level, Source or pair metadata fields, legacy2
wrappers, foreign target/version, receipt/metadata/hash tampering and missing
CLI/source authority all refuse. Full C7 build/provenance/release validation
remains with its existing owner; opaque build reports are never qualification.
No receipt rewriting, projected schema1 or invented source authority is allowed.
Original-byte typed deserialization is mandatory: no Value-map roundtrip that
collapses duplicate source/runtime keys. The schema2 checkpoint preflight uses
this SAME typed reader plus its existing complete payload hash pass; its old
schema1 path and schema2-with-runtime2 refusal stay unchanged.

Private fixtures gain only explicit preserve_pair_metadata=False by default.
The existing historical U93schema2 adapter remains byte-for-byte default. The
opt-in first bounds raw pair metadata4096/manifest512KiB/build16384 bytes before
its initial classifier, parsing or hashing. The schema3 fixture verifies the complete original pair and genuine C7build
receipt through the C7 owner and stages that ORIGINAL source directory. Its
inventory/runtime/build bytes are unchanged. Native U93 stage already admits
Host schema3: manifest_metadata -> RuntimePair.validate_target, final image
hashes and read_installed_pair. This fixture does not become modern I4 setup.

Strict native public control Health.source admission remains unchanged.
Null Health.build is retained and refused explicitly, never dereferenced or
accepted as a source match. A failed run retains StopComplete separately from
original process-handle exit observation; empty Job proof cannot invent a
missing handle wait. Failed historical records remain failed and immutable.

Regression scope: existing schema1 metadata/refusal tests unchanged; new C7
full-pair positive and unknown/missing/target/receipt/metadata/CLI/wrapper
refusals, plus opt-in original-byte fixture selection and default preservation.
No wire, digest input, persisted record or release-policy shape changes.

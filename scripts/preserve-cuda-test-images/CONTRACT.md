# CUDA lib-test image preservation

This maintained successor retains the reviewed c6a6e22a first-FP32 preserver's
copy and custody checks. It never builds, lists or executes native tests, loads a
DLL/model, signs, installs, activates or qualifies a model. It changes no ACL or
owner and never overwrites an existing output. Historical images and receipts
remain immutable.

The caller supplies exact reviewed script/guard, actual receipt/log, native-group,
source-commit and complete runtime-census hashes. The actual receipt must be
completed, successful and unchanged, with the exact two-crate all-feature lib
no-run CargoJSON command, now including `--offline`. The current incremental
setting is recorded exactly as `profile default; environment unset`; this is a
build-context observation, not a weaker source or artifact check. Static ORT
archive042c9701, environmenta348c538 and native manifestaf6d1ec5 remain pinned.
Cargo must emit one successful finish, one exact static ORT rc10 link row and
one GPU-featured lib-test artifact per Inference/Retrieval. The complete native
source roster is reopened before and after copying.

Root's additive destination decision retains the native-emitted manifest locator:
create exactly one NEW Inference `.exe` leaf in the existing protected runtime
directory under the approved human USER_ROOT. Do not recopy or edit its DLLs,
manifest, producer, notices or two historical executables. Authenticate the whole
original directory census before copying, then require exactly the same objects
plus the new independently copied image. A new census is a new observation;
historical census identities are not rewritten. Retrieval and inventory use one
fresh direct child of the human-only ARTIFACT_ROOT. No source hard links, aliases,
reparse points or destination clobber are allowed.

Both original and destination images require PE64 x64 executable headers. Every
source is single-link and human-owned; full byte hash, size, file ID, mtime, owner
and SDDL are retained and rechecked. Destination IDs must be independent. Runtime
payloads permit only the existing human/System/Administrators/TrustedInstaller
write policy; inventory outputs retain the stricter human-only write policy.
The maintained Windows guard checks effective/default owner, current interactive
identity, output parents and inherited access before every creation, and audits
each new output before it is consumed. Caller must first enter the verified
same-human execution path; the helper does not switch accounts or elevate.

`airk.preserved-native-test-images/1` remains the consumed image packet shape.
Separate bindings retain actual Cargo rows, static inputs, complete original
runtime census, the sole new runtime entry, original/destination custody and
optional bounded opaque caller compile-context. The compile-context proves only
its file bytes; actual runtime admission must validate compiled manifest/static
identity separately. Failure leaves partial outputs and an explicit nonpass;
there is no overwrite, cleanup or unchanged retry.

CLI: `python -S -B scripts/preserve-cuda-test-images.py --receipt PATH
--receipt-sha256 SHA --log PATH --log-sha256 SHA --native-group SHA
--source-commit COMMIT --approved-script-sha256 SHA --guard-sha256 SHA
--runtime EXISTING_RUNTIME --runtime-census-sha256 SHA --inference-name NEW.exe
--output FRESH_INVENTORY_DIRECTORY`. Optional compile context requires both
`--compile-input-context PATH` and `--compile-input-context-sha256 SHA`.

The runtime-census hash is SHA256 of this owner's `runtime_census` result encoded
by `census_digest` (canonical sorted-key JSON with compact separators). Preparing
that read-only census dispatches no native work. Tests use literal/in-memory
fixtures; they do not qualify live CUDA artifacts.
Each runtime census retains a fresh full-byte hash and stat/file-ID observation
for every file. Its read-only ACL observations use one PowerShell process and the
maintained guard's bounded batch, with an original ten-second process deadline
and current-token checks for every metadata path. Owner SID, SDDL and every ACE
remain exact. Partial, duplicate, reordered or changed paths, metadata errors,
audit refusals and expiry fail closed without retry. Every file's canonical path,
stat and file ID, and the directory membership, are reobserved after the batch.
There is no policy cache across phases: the initial census and both complete
reopen censuses remain, as do both full native-source passes. Creation audits,
copy-only semantics and all non-runtime policy observations are unchanged.
The read-only census API is now `runtime_census(runtime, reviewed_guard_sha256)`.

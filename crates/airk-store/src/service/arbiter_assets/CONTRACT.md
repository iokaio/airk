# Arbiter asset transaction contract

Store owns the separate private `arbiter-v3/assets/<manifest>/` tree under an actual retained ServiceLease. Opening, absent inspection and disabled startup perform no writes. Paths come only from validated Core Profile asset names and compiled manifest identity; ordinary provisioning accepts no caller directory/URL. Product service validates release qualification before supplying a profile; the separate typed private evaluation fixture supplies an opaque evaluation capability and never product qualification. No index/I4 bytes change. Configuration/3 is the explicitly new optional state contract in [configuration custody](../arbiter/CONTRACT.md); older state remains untouched.

Explicit provisioning takes an actual descriptor-relative per-profile writer lock, writes a canonical installation/catalog/profile/configuration-revision-bound receipt, and streams data into a fresh stage generation. Each logical asset has an exact length and full SHA-256. Only complete verified files publish with no replacement. The Ready receipt publishes only after the complete approved closure verifies. Publication and receipt writes retain actual descriptors, sync bytes/directories, and compare prior receipt identity before replacement. A same-name foreign/linked/mismatched file is preserved and refused.

The private receipt schema `airk.arbiter.assets/3` binds installation/catalog/manifest, monotonic asset revision, originating configuration revision, phase and complete Core asset roster. A crash or canceled provisioning never becomes Ready from its filename or mere existence. Explicit recovery uses the same compiled profile, verifies any already-published files, and creates a new stage generation for remaining assets. Interrupted stage directories are retained as recovery evidence, never executed, used as a download authority or silently deleted. Ready logical files are read through retained descriptors and rehashed; pins retain the actual service lease. Removal refuses live pins, deletes only verified declared final files and publishes an exact Removed receipt; retained intent metadata and recovery evidence are disclosed and preserved.

Downloads are a service adapter responsibility. Core's optional AssetDownload/AssetPart list is compiled authority: every asset has one ordered part roster, each part <=1GiB with exact SHA/length and an exact official iokaio/airk GitHub release URL. Full logical concatenation length/hash must still match Asset. Legacy profiles omit the list unchanged; product provisioning requires complete qualified closure. No downloaded manifest, teacher label, candidate artifact or filesystem path can mint release qualification.

Partial settlement uses private `airk.arbiter.partial/3` receipts: the exact original canonical operation, logical Asset declaration, actual written length/SHA and native file identity are sealed while the descriptor remains owned. Explicit removal may delete only matching declared final files and these proven partial bytes, under real writer/reader locks. Intent metadata and interrupted receipt stages remain retained. A hard crash before partial sealing, a changed file/receipt, unknown entry or foreign identity preserves the bytes and leaves Removing/refusal; it is never manufactured into successful cleanup.

Explicit Remove may also clean a withdrawn profile using only a valid installation/catalog/manifest-bound owned receipt in this fixed content-addressed root, exact safe native filenames, actual bytes/hash/identity and no-follow/pin checks. That cleanup observation is neither an execution capability nor a replacement catalog. It cannot warm, download, enroll or promote a model. A matching unfinished removal is rediscovered only for its original configuration revision and remains a removal.

Windows removal reuses Process opened-directory pins and verifies their actual identities against the retained Store root and each directory. Its existing remove_file_if adapter opens one unredirected single-linked READ|DELETE file with shareREAD only, rejects extra streams through the one shared guard, hashes default bytes and applies disposition on that same handle. No separate unlink can race the proof, and an existing writer, changed bytes, hardlink or alternate stream refuses with foreign content preserved. The reviewed Windows-only Store→Process foundation edge adds no service or execution policy to Process. Unix retains its existing descriptor-relative behavior and does not claim Windows share exclusion.


The approved version 3 private asset receipt also records each final file's original native object identity. Custody is durably recorded while the completed staged writer is still held, BEFORE no-replace publication. Ready, pinning, reuse and removal require that identity as well as complete bytes/hash; same-byte replacement is foreign and preserved. Phase settlement carries the custody map forward after interruption. A prior receipt cannot mint missing object or stage ownership and is refused/preserved; no product profile, I4 or source index format changes. Partial version 3 embeds the exact version 3 original operation, retaining original staged identity for interrupted pre-publication cleanup.

The new assets/3 receipt additionally binds explicit Focus/Context stage and
configuration_schema=airk.arbiter.configuration/3. Current profiles must declare
that same stage before any transaction; profile reads, pinning and recovery
recheck it. Assets/2 and partial/2 are never interpreted as current v3 custody.
The retained arbiter-v1/v2 roots are not adopted or modified by absent inspection,
new provisioning or explicit disabled v3 initialization. Recovery filters by
original stage as well as original configuration revision.


Owner-only offline import accepts an actual compiled qualified profile and the
original native absolute local directory. It opens every ancestor without
following redirection, retains readonly regular single-linked source descriptors,
rejects extra streams on Windows, positively observes a supported local volume,
and requires exactly the compiled asset roster. A bundle supplies no profile,
compiler code, URL, enrollment, consent or qualification authority. No source
object is written. Windows reuses existing Process ancestor/share pins; Unix
reuses Store descriptor-relative directory/file opens, with no new crate edge.

Before destination effects, every source asset receives a complete streamed
length/SHA check. Source directory/file identities and roster are rechecked;
copy uses those retained descriptors. The ordinary transaction owns destination
writer exclusion, original object custody, partial recovery and no-replace
publication. Even cached destinations require the actual supplied source bytes
to verify. Cancellation is checked between bounded 64KiB reads, preserves source
bytes and settles the original transaction without reporting Ready. Source
mutation, aliases, unknown/nonlocal volumes and unknown entries refuse. Actual
publication cannot be undone by a later cancellation; its original owned files
and recovery receipt remain explicit. Only the complete verified closure can
settle Ready. Import itself never enables or enrolls a stage or starts a download.

### Same-operation failed source restart

`Transaction::restart_failed_asset_if(writer, check)` consumes only its own unpublished writer for one declared logical asset under the existing original operation and writer exclusion. It positively seals the existing partial/3 record, closes that writer, and reopens the exact operation, partial proof and staged object. Original stage identity, ServiceLease identity, operation/configuration revision, asset declaration, native object identity, actual length and complete partial hash must still match. Only those authenticated unfinished bytes and their proof may be unlinked through the existing platform custody path; a fresh CreateNew writer begins at zero. Final cached files, all other assets, older stage generations and receipts are preserved. Foreign/replaced/linked/unsealed bytes refuse without adoption or truncation. Failure cannot claim Ready, deletion or successful restart.

This is local transaction custody only: it grants no network, retry, deadline, scope, consent or configuration authority. Service admits a backup only within its original bounded source attempt/per-part deadline/cancellation contract and retains sanitized failed-source diagnostics. A failed seal or local custody check stops, and the original file/evidence remains for explicit recovery. The internal sealed-writer bit is not persisted and does not pretend that a partial asset was published. Existing assets/3 and partial/3 serialized bytes are unchanged.

Production callers use `restart_failed_asset_if(writer, check)` with an original-clock/atomic-cancellation-only admission predicate; the callback acquires no lifecycle, configuration or Store locks and authorizes no I/O/network itself. Entry and post-seal checks run before cleanup. The last predicate runs after full actual partial-byte hashing, immediately before first native deletion; Windows holds the authenticated proof and asset write/delete exclusions there. An observed cancellation/expired original source deadline before that fence refuses with the partial/proof and completed cache preserved. Native cleanup already committed before a later cancellation is not fabricated into rollback; no fresh writer/network is admitted after the final predicate. The unchecked one-argument convenience exists only inside owning unit tests.

## D08 separate assets4 and partial4 generation

AssetStore::open retains exact legacy arbiter-v3/assets and assets3/partial3/configuration3 serialization; it cannot mint new GPU custody. AssetStore::open_device selects ONLY fixed protected arbiter-v4/assets and canonical assets4/partial4 bound to configuration4. Both reuse the same existing descriptor-relative locks, original file-identity custody, no-replace staging/publication, bounded full hashes, actual ServiceLease/pins and verified Windows/Unix removal paths. A generation is not caller path authority. No receipt/partial/asset is silently relabeled, moved, hardlinked or adopted between roots. Legacy CPU assets3 remain readable/pinnable only through the original compiled CPU profile custody; service current operations must separately choose that honest compatibility route. No GPU descriptor can acquire legacy3 custody. New explicit transactions use4.

Stage intents must match the receipt generation, configuration schema and stage. Partial schema derives only from a valid (assets3,configuration3) or (assets4,configuration4) pair; mixed/unknown pairs refuse. Actual partial object identities/hashes, cancellation, recovery and removal are unchanged. The original cleanup configuration-revision semantics remain: an explicit later authorized Remove may clean an earlier owned stage; this is not an old work grant or native execution capability. Native scopes/consent/compute qualification stay in their existing owning service/inference gates.

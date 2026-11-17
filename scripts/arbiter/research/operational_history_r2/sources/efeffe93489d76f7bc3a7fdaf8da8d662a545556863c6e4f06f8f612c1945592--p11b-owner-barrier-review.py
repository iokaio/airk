# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
root=Path('D:/code/Github/airk'); base=root/'artifacts/v190/p10-worktree'; out=root/'artifacts/v190/p11b-owner-barrier-proposed'
baseline=json.loads((out/'baseline.json').read_text(encoding='utf-8'))
chunks=[]
for path,digest in baseline.items():
    actual=(base/path).read_bytes()
    assert hashlib.sha256(actual).hexdigest()==digest,path
    chunks.extend(difflib.unified_diff(actual.decode('utf-8').splitlines(True),(out/path).read_text(encoding='utf-8').splitlines(True),fromfile='a/'+path,tofile='b/'+path))
patch=''.join(chunks)
(out/'owner-barrier.patch').write_text(patch,encoding='utf-8',newline='\n')
notes={
'status':'INERT reviewable proposal, not applied or accepted. No Cargo/compiler/tests/SSH/tracked edits.',
'patch_sha256':hashlib.sha256(patch.encode()).hexdigest(),
'reviewed_foundation':'No identity recursion found: identity -> AppliedRetention::from_profile -> legacy IndexDetail::from_profile; valid historical profiles retain framing/serialization; contract3 differs explicitly. Complete producer equality includes grammar, mapping, chunk/options/configuration, not only adapter version. Migration review is pure and requires owning native policy/target recomputation before authority.',
'actual_old_profile_refusal':[
'Release v1.8.7 git show and frozen P9: CurrentRead::captured_input invokes CapturedInput::bytes -> profile.identity; file_metadata authenticates artifact then metadata::verify -> profile.identity. Contract3 therefore refuses that file before its verified facts are returned.',
'CollectionConfiguration deny_unknown_fields refuses added applied-retention configuration when collection_state is decoded. These checks are insufficient as a universal entry fence because raw map/head reads may not decode any artifact or configuration.',
'Release v1.8.7 and frozen P9 current_read_once acquire shared current Gate then check_owner_with_projection -> read_owner -> bounded current-owner.json Owner deny_unknown_fields. They do not reread stable service-owner.json for cached FileStore access. Only isolated P10 added validate_split_store_access.'],
'activation_order':[
'Native reviewed Configure must release its own request/Selection pins, hold the exact selected store writer and matching quota/funding, and call Collection::require_extraction_contract(...,3) BEFORE policy4 or contract3 output.',
'Method holds manifest exclusion, verifies stable binding, exact active slot, actual current scope/root/collection, then takes EXCLUSIVE current Gate. An actual old CurrentRead/selection keeps shared Gate and refuses activation without changing either marker.',
'Allocate and write bounded lower-owner temporary bytes before stable publication; persist stable capability first, then current-owner capability; return only after both. Old compatible publication/token/source bytes remain untouched.',
'Crash between markers cannot authorize new interpretation: migration caller has not received success or published policy4. Retry repeats exact3. Future current owner creation reads actual stable Owner, including from cached new code, not cached Collection.owner.',
'After lower marker publication, an old cached FileStore acquires its shared Gate but unknown-field parsing refuses before opening current SQLite. Existing writer growth/projection updates preserve typed marker. Old replacement commit checks actual previous current scope and cannot swap a fresh unmarked slot over marked active owner.',
'Unsupported values refuse typed deserialization; missing fields preserve historical serialized bytes. Stable marker excluded from immutable owner binding; per-slot header has no independent immutable digest to change.'],
'native_process_limit':'NativeOwnerSession::settle_after_owned_drain proves only its own actual accepted work/LocalTools admission joined and durable session closed. ProjectLease/native process ownership does not enumerate arbitrary direct cached FileStore readers. Actual current Gate provides their OS-backed exclusion; no claim of global arbitrary process census.',
'regression_proposed':'Actual Store fixture initializes real current/lexical publication, holds real CurrentRead to prove marker activation defers and bytes unchanged; drops reader, installs exact3 idempotently, proves token/binding/source unchanged, exact predecessor Owner parser refuses actual header, fresh new slot through cached collection inherits marker, unselected slot/contract4 refuse. This models predecessor parser in-process, not an executed historical binary.',
'unresolved_acceptance_requirements':[
'Native reviewed saved Configure/policy4 wiring is peer-owned and must actually call seam before desired policy/new bytes. Unmanaged/legacy non-service migration remains explicitly unsupported.',
'The inert patch has only rustfmt parse/format verification, no typecheck or test evidence. Actual native Kernel migration, abrupt crash between stable/lower owner, quota failure, and actual old executable compatibility tests remain required before activation/acceptance.',
'Existing retirement/recovery/reset callers must be included in final combined review. Lower predecessor read_owner_any is deny_unknown_fields, including legacy-retirement scope; supported writers do not silently discard unrecognized header.',
'No OCR adapter/dependency/model/provisioning changes. No schema-version or renderer parity claim from this owner-only proposal.'],
'citations':{
'P9 root':'current/read.rs476+, captured_input331+; current/journal.rs CapturedInput::bytes230+; current/metadata.rs656+, verify678+; current/collection.rs50+; current/database.rs42+,check_owner_with_projection53+,read_owner_any119+; current/gate.rs144+; service_collection_native_owner.rs371+',
'isolated P10':'current/read.rs481 validate_split_store_access; current/journal.rs1070 same; service_collection_split.rs403+ live Owner decode; service_collection.rs79+ Owner,94+ owner_digest,769+ existing,822+ select; current/database.rs43+ Owner,read_owner_any119+ (some line offsets changed by earlier P10 pins)'
},
'validation':'Five ignored full source copies parsed/formatted with C:/Users/tdotn/.cargo/bin/rustfmt.exe --edition2024 --config skip_children=true; baseline hashes checked before regenerating formatted patch. No source/compiler-input mutation.'
}
(out/'independent-review.json').write_text(json.dumps(notes,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'files':len(baseline),'patch_sha256':notes['patch_sha256']}))

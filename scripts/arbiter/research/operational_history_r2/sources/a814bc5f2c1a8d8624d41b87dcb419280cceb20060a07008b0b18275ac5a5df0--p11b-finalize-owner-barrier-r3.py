# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
root=Path('D:/code/Github/airk');base=root/'artifacts/v190/p10-worktree';out=root/'artifacts/v190/p11b-owner-barrier-r3-proposed'
baseline=json.loads((out/'baseline.json').read_text(encoding='utf-8'));chunks=[]
for path,digest in baseline.items():
    raw=(base/path).read_bytes() if (base/path).exists() else None
    assert (hashlib.sha256(raw).hexdigest() if raw is not None else None)==digest,path
    chunks.append('diff --git a/'+path+' b/'+path+'\n')
    if raw is None:chunks.append('new file mode 100644\n')
    chunks.extend(difflib.unified_diff((raw.decode('utf-8') if raw is not None else '').splitlines(True),(out/path).read_text(encoding='utf-8').splitlines(True),fromfile='a/'+path if raw is not None else '/dev/null',tofile='b/'+path))
patch=''.join(chunks);(out/'owner-barrier-r3.patch').write_text(patch,encoding='utf-8',newline='\n')
notes=json.loads((root/'artifacts/v190/p11b-owner-barrier-r2-proposed/notes.json').read_text(encoding='utf-8'))
notes.update(status='INERT r3 correction of independently reviewed r2 blockers. V1/V2 preserved; no tracked edits, compiler/Cargo/native execution or SSH. Not accepted.',
             patch_sha256=hashlib.sha256(patch.encode()).hexdigest(),files=list(baseline))
notes['contracts']['scratch']='ALL prepared marker bytes now live exclusively inside actual Quota Reservation.staging_directory. Partial write/abrupt exit remains in existing ledger-owned staging with an actual lease, rather than unknown fixed slot/root leaves. First-order ordinary recovery reclaims dead owned partial staging before a new reservation; funded stage credit/custody remains governed by existing exact claimed takeover +actual Ready/absent/disposed settlement, never generic early release. No new root/slot pending-name allowlist or retirement interpretation. Temporary Drop removes only exact source object; reservation stays held through marker publication.'
notes['contracts']['anchoring']='Retained stable/lower expected file handles and all bounded slot Gates unchanged. New narrow SetupDirectory::replace_file_to checks exact source and expected destination file identities then performs source-directory->destination-directory anchored rename. Existing replace_file delegates same-directory behavior. Windows reuses existing publish_named Ex(REPLACE_IF_EXISTS|POSIX_SEMANTICS), no ACL/share/readonly relaxation. Unix renameat uses both retained descriptors; no inode-CAS claim. Source is actual quota staging; destination remains captured native directory.'
notes['r2_blockers_closed_prospectively']=[
'Supported abrupt mid-write no longer leaves a malformed unknown fixed pending file outside custody; exact paid staging owns partial bytes. No public/native authority from its mere existence.',
'Stable Owner new encoding is bounded <=MAX_METADATA16384 immediately after validating existing owner and BEFORE any preparation/capability effect. Growth past format bound refuses rather than publishing unreadable metadata. Lower existing65,536 bound retained.'
]
notes['tests_proposed']=[
'Preserved real active-reader/old parser/new-slot inheritance regression and real abandoned replacement all-slot reader/fault/retry/recovery test. Ordinary interrupted marker preparation compares actual reserved credit before/after rather than checking implementation-specific temporary names.',
'New actual abrupt subprocess test uses existing Store ChildGuard, exact ignored child entry requiring owned env, file stdout/stderr and30s try_wait bound. Child opens actual existing native collection/writer+Quota, charges actual reservation, enters real PreparedMarker staging constructor then writes/fsyncs one byte and exits79 without Drop. Parent proves actual partial file exists inside exact reported owned stage; retries native barrier, which triggers actual dead reservation recovery; stage gone, publication token/source bytes unchanged. Child has no descendant-spawning path; no unbounded pipes/new process framework. This test is UNRUN.',
'Exact stable-owner serialized-bound unit starts from native initial-owner encoding, pads root scalar so old canonical encoding is exactly16384, and verifies required-field encoding refuses without mutating original. This is a pure bound check, not fabricated native publication or source provenance.',
'Unknown incarnation source file stays unchanged and prevents capability effects. Peer guard/native/inheritance proposals still supply selected-config/no-state bypass and real paid Lean/Rich lifecycle checks; those remain unrun and need composed review/compile.'
]
notes['limits']=[row for row in notes['limits'] if 'Hard crash during' not in row]
notes['limits'].extend([
'New ordinary abrupt partial-stage test is proposed only. Funded abrupt marker-stage recovery must be exercised through existing actual claimed worker retry/cancel custody+Ready/absent/disposed boundaries; this patch does not invent a new release proof or claim it ran.',
'Cross-directory replacement implementation reuses existing tested primitives, but new actual platform compilation/tests remain required. Helper ignored entry is not counted as passing routine coverage.',
'Compose additively with peer central guard/native-r3/inheritance-r3. New stable accessor and marker fields same API, but database/journal/tests overlaps require exact hunks; no whole-file overwrite.'
])
notes['validation']='11 ignored Rust copies rustfmt parsed/formatted; live baseline SHA256 verified; proper new-file Git metadata. No Rust typecheck/test/native child execution. Read-only apply-check to collect next.'
(out/'notes.json').write_text(json.dumps(notes,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'files':len(baseline),'patch_sha256':notes['patch_sha256']}))

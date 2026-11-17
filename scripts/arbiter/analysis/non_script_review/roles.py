# SPDX-License-Identifier: Apache-2.0
"""Compact positive-role review; never claims exact historical build reproduction."""
import argparse
from collections import Counter
import hashlib
import json
import re
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'upstream_tools'))
import project_archives as inputs
ROOT=Path(__file__).resolve().parents[4]
LEGACY='scripts/arbiter/research/operational_history_r2/sources/3b6aebddf4407da3c0e12c6d40106e58b068336c130718ebc7c7a7ee0398642e--p6f-resume-legacy-upgrade-editor-owner.py'
UPGRADE='scripts/arbiter/research/operational_history_r2/sources/4b66e350ea1870b6c82b39a14b4ee7df84590bdcb1f2d4099a525cc52a937783--verify-upgrade.cjs'
AUTH='scripts/arbiter/research/operational_history_r2/sources/a0d8cdf3366349a1c8665d5749f9dcd4f456245e770a777d9f92860e4c0eccf7--fts5-final-check.py'


DETAIL={
 '4a7bb1e6fc1ab4b78b0dbb96a6d793cd961d7e43e404e2abd18b78dd8b57929d':('one','84fb1e190f3c83da9b07933e9148469804c222f622d331dbd21172e3a2424f47'),
 'f69a46a8af21e1e9bb59fc1105b9d763b4fda48cfc23e842e37d2078c5b5c5d9':('two','fd4d566fcf3f6d4068000163c1ee8b9cf00adf34e9a023038d6be39a138a5716')}
RUST_FETCH='scripts/arbiter/research/operational_history_r2/sources/5633f333668a2a4abb2c805d820490caa77d0e99566e258012fb1dc2310dc341--fetch-rust-mac-clock-source.py'

def detail_evidence(digest):
    label,generation=DETAIL[digest]
    path=f'artifacts/detail-harness-smoke-3/state/balanced/collections/comparison-{label}-balanced/generations/{generation}/lineage.json'
    raw,pin=inputs.consumed(inputs.checked(ROOT,path));value=inputs.strict_json(raw)
    matches=[e.get('after') for e in value['events'] if e.get('after') and e['after'].get('source_hash')==digest and e['after'].get('path')==label+'.rs' and e['after'].get('root','').replace('\\','/').endswith('/artifacts/detail-harness-fixtures/'+label)]
    if not matches:raise ValueError('indexed input lineage does not bind exact source')
    return dict(path=path,**pin)


def role(path,raw):
    digest=hashlib.sha256(raw).hexdigest()
    if digest in DETAIL and path.startswith(('artifacts/detail-harness-','artifacts/enhancements-final-harness-')):
        return 'observed_indexed_harness_input','scripts/benchmark-detail.py',['--incremental-copies','inventory','copied = output / "copies"']
    if path=='artifacts/advanced-split/receipts/mac-platform-scoped-attempt4/rust-1.98-unix-time.rs' and hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()=='944cb552cad9ef84348dc6ea35287a60bd2453c1':
        return 'upstream_reference_input',RUST_FETCH,['88d9e12ae178fab0fb5cc050a94da85685d449ea','944cb552cad9ef84348dc6ea35287a60bd2453c1','library/std/src/sys/time/unix.rs']
    # Normalization below identifies documented synthetic fixture roles only;
    # raw hashes always retain the actual original bytes.
    text=raw.replace(b'\r\n',b'\n')
    native={b'export function WindowAlphaEvidence() { return 42; }\n',b'export function WindowBetaEvidence() { return 42; }\n',b'export function SharedAllowedEvidence() { return 19; }\n',b'export function HiddenPrivateEvidence() {}\n',b'export function WindowAlphaEvidence() { return 42; }\n\nexport function SharedEditEvidence() { return 7; }\n'}
    if path.endswith('.ts') and text in native:
        return 'generated_indexed_fixture','scripts/check-native-editor.cjs',['fs.writeFile','WindowAlphaEvidence','SharedAllowedEvidence','SharedEditEvidence']
    if path.endswith('.ts') and text==b'export const InstallerEvidence = 42;\n':
        return 'generated_indexed_fixture','scripts/check-agent-install.cjs',['fs.writeFile','InstallerEvidence']
    if path.endswith('.ts') and text==b'export function LegacyOriginalEvidence() { return 42; }\n':
        return 'generated_indexed_fixture',LEGACY,['write_text','LegacyOriginalEvidence']
    if path.endswith('.rs') and text==b'fn release_upgrade_fixture() {}\n':
        return 'generated_indexed_fixture',UPGRADE,['writeFile','release_upgrade_fixture']
    if path.endswith('.cs') and text==b'class AuthConfig { public void ConfigureAuthentication() {} }\n':
        return 'generated_indexed_fixture',AUTH,['write_text','ConfigureAuthentication']
    if path.startswith('artifacts/mcp-optimizations-1.8.3/frozen-fixture/source/') and all(x in text for x in [b'SPDX-License-Identifier: Apache-2.0',b'Session']) and (b'Fictional ' in text or b'OwnerSession' in text or b'WorkbenchSession' in text):
        return 'generated_indexed_fixture','scripts/benchmark-mcp-investigation.py',['def create_fixture','Fictional','McpStdioSession']
    if '/installer-' in path and path.endswith('/stub.cs') and all(x in text for x in [b'class Stub',b'__airk_role',b'observed-operation.json',b'inert']):
        return 'generated_inert_installer_payload','scripts/check-windows-installer-wrapper.py',['STUB =', 'code.write_text(STUB.replace', 'observed-operation.json']
    if path.startswith('artifacts/readiness-1817/host-fixtures/') and Path(path).name.startswith('fixture_'):
        match=re.fullmatch(r'fixture_(\d+)\.rs',Path(path).name)
        if match and 0<=int(match[1])<32:
            expected=''.join(f'/// Original measurement fixture.\npub fn fixture_{int(match[1])}_{item}(value: usize) -> usize {{ value + {item} }}\n' for item in range(256)).encode()
            if text==expected:return 'generated_indexed_fixture','crates/airk-kernel/src/usage.rs',['compact_measurements_compare_small_reads_from_large_catalog','Original measurement fixture.','for file in 0..32']
    if '/build/airk-' in path and path.endswith('/out/bundled_model.rs') and all(x in text for x in [b'pub type EmbeddedProfileFiles',b'pub static PROFILES']):
        return 'generated_native_embedded_asset_table','crates/airk-host/build.rs',['EmbeddedProfileFiles','PROFILES','bundled_model.rs']
    if '/build/airk-https-' in path and path.endswith('/out/admin_assets.rs') and all(x in text for x in [b'pub static ADMIN_ASSETS',b'include_bytes!']):
        return 'generated_native_embedded_asset_table','crates/airk-https/build.rs',['ADMIN_ASSETS','admin_assets.rs','include_bytes!']
    if '/copies/airk/' in path and path.startswith(('artifacts/detail-real-final/','artifacts/enhancements-qualified-final/','artifacts/enhancements-real-final/')) and b'SPDX-License-Identifier: Apache-2.0' in text[:200]:
        return 'indexed_workspace_copy','scripts/benchmark-detail.py',['--incremental-copies','copied = output / "copies"','workspace["label"]']
    return None


def project(residual):
    raw,pin=inputs.consumed(residual,256<<20);data=inputs.strict_json(raw)
    rows=[r for r in data['remaining'] if r['lane']=='component_review'];files=[];unresolved=[];owners={}
    for row in rows:
        body,actual=inputs.consumed(inputs.checked(ROOT,row['path']))
        if actual!={'sha256':row['source_sha256'],'bytes':row['bytes']}:raise ValueError('frozen candidate changed: '+row['path'])
        match=role(row['path'],body)
        if match is None:unresolved.append(row);continue
        label,producer,anchors=match
        if producer not in owners:
            producer_raw,producer_pin=inputs.consumed(inputs.checked(ROOT,producer));owners[producer]=(producer_raw,dict(path=producer,**producer_pin))
        producer_raw,producer_pin=owners[producer]
        if any(anchor.encode() not in producer_raw for anchor in anchors):raise ValueError('producer anchor absent: '+producer)
        extra=[detail_evidence(actual['sha256'])] if label=='observed_indexed_harness_input' else []
        files.append(dict(evidence=extra,owner_relation='maintained benchmark consumer; original fixture generator not claimed' if extra else 'maintained producer or acquisition owner',source=row['path'],source_sha256=actual['sha256'],source_bytes=actual['bytes'],role=label,producer=producer_pin,producer_anchors=anchors,source_pattern_checked=True,exact_historical_output_reproduced=False,limitation='Positive generated/input role and maintained owning implementation; historical execution, parameters and full-output identity not reconstructed. No source copied or executed.'))
    for path,(before,pin) in owners.items():
        if inputs.consumed(inputs.checked(ROOT,path))[0]!=before:raise ValueError('producer changed during review')
    return dict(schema='airk.private.source-role-review/1',input=dict(path=residual.relative_to(ROOT).as_posix(),sha256=hashlib.sha256(raw).hexdigest(),bytes=len(raw)),source= dict(path=Path(__file__).resolve().relative_to(ROOT).as_posix(),**inputs.measure(Path(__file__))),files=files,unresolved=unresolved,counts=dict(Counter(r['role'] for r in files)),role_only=True,exact_historical_identity_complete=False,qualification=False)


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--residual',required=True,type=Path);args=p.parse_args();print(json.dumps(project(args.residual.resolve()),separators=(',',':')))

if __name__=='__main__':main()

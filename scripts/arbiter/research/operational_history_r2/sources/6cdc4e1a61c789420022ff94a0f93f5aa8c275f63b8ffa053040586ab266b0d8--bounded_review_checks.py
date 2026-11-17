# SPDX-License-Identifier: Apache-2.0
"""Frozen source/publisher controls; no Cargo, TLS, network or upstream codegen."""
import ast, base64, datetime, hashlib, importlib.util, json, re, tomllib
from cryptography import x509
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
HERE=Path(__file__).resolve().parent
CACHE=Path.home()/'.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/webpki-roots-1.0.9'
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def der_contents(data):
    assert data[0]==48
    n=data[1]; offset=2
    if n&128:
        count=n&127; n=int.from_bytes(data[2:2+count],'big');offset+=count
    assert len(data)==offset+n
    return data[offset:]
def main():
    provenance=json.loads((ROOT/'artifacts/advanced-split/local-delivery-preparation-r1/webpki-roots-source-provenance.json').read_text(encoding='utf-8'))
    for row in provenance['packaged_files']: assert sha(CACHE/row['path'])==row['sha256']
    upstream=ROOT/'artifacts/tools/cargo-vet/mozilla-audits.toml'
    assert sha(upstream)=='77617e6e8d24664ed21af86b9e112489e37740280fafc293bc3a417449e87a4b'
    trust=tomllib.loads(upstream.read_text(encoding='utf-8'))['trusted']['webpki-roots'][0]
    version=json.loads((HERE/'crates-io-1.0.9.json').read_text(encoding='utf-8'))['version']
    assert version['published_by']['id']==trust['user-id']==2751
    assert trust['criteria']=='safe-to-deploy' and trust['start']<=version['created_at'][:10]<trust['end']
    assert version['checksum']==provenance['identity']['checksum'] and not version['yanked']
    src=(CACHE/'src/lib.rs').read_text(encoding='utf-8')
    assert '#![no_std]' in src and '#![forbid(unsafe_code, unstable_features)]' in src
    blocks=re.findall(r'/\*(.*?)\*/\s*TrustAnchor\s*\{(.*?)\n  \}',src,re.S)
    assert len(blocks)==src.count('  TrustAnchor {')==121
    fingerprints=[]
    for comment,anchor in blocks:
        fp=re.search(r'SHA256 Fingerprint: ([0-9a-f:]+)',comment).group(1).replace(':','')
        pem=re.search(r'BEGIN CERTIFICATE-----\s*(.*?)\s*\* -----END CERTIFICATE',comment,re.S).group(1)
        der=base64.b64decode(re.sub(r'[\s*]','',pem),validate=True)
        assert hashlib.sha256(der).hexdigest()==fp
        fingerprints.append(fp)
        cert=x509.load_der_x509_certificate(der)
        fields=re.findall(r'(subject|subject_public_key_info): Der::from_slice\((b"(?:[^"\\]|\\.)*")\)',anchor)
        actual={name:ast.literal_eval(literal) for name,literal in fields}
        assert actual['subject']==der_contents(cert.subject.public_bytes())
        assert actual['subject_public_key_info']==der_contents(cert.public_key().public_bytes(Encoding.DER,PublicFormat.SubjectPublicKeyInfo))
        assert 'subject: Der::from_slice(b"' in anchor and 'subject_public_key_info: Der::from_slice(b"' in anchor
    assert len(set(fingerprints))==121
    manifest=tomllib.loads((CACHE/'Cargo.toml').read_text(encoding='utf-8'))
    assert manifest['package']['build'] is False and set(manifest['dependencies'])=={'pki-types'}
    assert manifest['dependencies']['pki-types']['default-features'] is False
    spec=importlib.util.spec_from_file_location('source_guard',ROOT/'supply-chain/guard.py')
    guard=importlib.util.module_from_spec(spec);spec.loader.exec_module(guard)
    lock=ROOT/'Cargo.lock'
    try: guard.validate_lock_sources(lock,ROOT/'supply-chain/source-policy.json')
    except ValueError as error: assert 'webpki-roots 1.0.9' in str(error)
    else: raise AssertionError('baseline unexpectedly admitted new identity')
    result=guard.validate_lock_sources(lock,HERE/'source-policy.json.proposed')
    assert result['conclusion']=='success'
    bad=json.loads((HERE/'source-policy.json.proposed').read_text(encoding='utf-8'))
    next(x for x in bad['packages'] if x['name']=='webpki-roots' and x['version']=='1.0.9')['checksum']='0'*64
    negative=HERE/'wrong-checksum.policy.json';negative.write_text(json.dumps(bad),encoding='utf-8')
    try: guard.validate_lock_sources(lock,negative)
    except ValueError as error: assert 'webpki-roots 1.0.9' in str(error)
    else: raise AssertionError('changed checksum accepted')
    receipt={'schema':'airk.private-webpki-source-review-checks/1','controls_passed':8,'anchors':121,'cargo_or_network_executed':False,'certificate_trust_or_tls_qualification':False,'guard':result,'inputs':{str(x.relative_to(ROOT)):sha(x) for x in [lock,ROOT/'supply-chain/source-policy.json',HERE/'source-policy.json.proposed',HERE/'crates-io-1.0.9.json',upstream]}}
    (HERE/'bounded-checks-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(receipt,indent=2))
if __name__=='__main__': main()

# SPDX-License-Identifier: Apache-2.0
"""Read-only extension identity/signature/tree-hash projection; no browser execution."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import re
from project_archives import REPOSITORY, SOURCE_LIMIT, ARCHIVE_LIMIT, checked, measure, pinned, consumed, strict_json

SPKI_PREFIX=bytes.fromhex('30820122300d06092a864886f70d01010105000382010f003082010a0282010100')
DIGEST_INFO=bytes.fromhex('3031300d060960864801650304020105000420')
NAMES={'content.js','content_new.js','offscreendocument_main.js','page_embed_script.js','service_worker_bin_prod.js'}

def decode(value):
    if not isinstance(value,str) or not re.fullmatch('[A-Za-z0-9_-]*',value):raise ValueError('canonical unpadded base64url required')
    return base64.urlsafe_b64decode(value+'='*(-len(value)%4))

def rsa_key(der):
    if len(der)!=294 or not der.startswith(SPKI_PREFIX) or der[289:]!=bytes.fromhex('0203010001'):raise ValueError('exact bounded RSA2048 SPKI required')
    n=int.from_bytes(der[33:289],'big')
    if n.bit_length()!=2048:raise ValueError('RSA modulus width differs')
    return n

def verify_signature(signed,signature,der):
    n=rsa_key(der)
    if json.loads(decode(signature['protected']))!={'alg':'RS256'}:raise ValueError('only original RS256 signature accepted')
    raw=decode(signature['signature'])
    if len(raw)!=256 or int.from_bytes(raw,'big')>=n:raise ValueError('RSA signature width/value differs')
    value=(signature['protected']+'.'+signed['payload']).encode('ascii')
    suffix=DIGEST_INFO+hashlib.sha256(value).digest()
    expected=b'\x00\x01'+b'\xff'*(256-len(suffix)-3)+b'\x00'+suffix
    if pow(int.from_bytes(raw,'big'),65537,n).to_bytes(256,'big')!=expected:raise ValueError('retained signature does not verify')

def tree_hash(raw,block_size,hash_block_size):
    if block_size!=4096 or hash_block_size!=4096:raise ValueError('original tree block sizes required')
    nodes=[hashlib.sha256(raw[i:i+block_size]).digest() for i in range(0,len(raw),block_size)] or [hashlib.sha256(b'').digest()]
    while len(nodes)>1:nodes=[hashlib.sha256(b''.join(nodes[i:i+128])).digest() for i in range(0,len(nodes),128)]
    return nodes[0]

def observed_webstore_key(catalog):
    p=Path(catalog['binary']['path'])
    if not p.is_absolute():raise ValueError('absolute observed runtime path required')
    for part in [p,*p.parents]:
        if part.is_symlink() or getattr(part.lstat(),'st_file_attributes',0)&0x400:raise ValueError('aliased observed runtime refused')
    if measure(p,ARCHIVE_LIMIT)!={k:catalog['binary'][k] for k in ('sha256','bytes')}:raise ValueError('observed browser runtime changed')
    with p.open('rb') as f:f.seek(catalog['offset']);der=f.read(catalog['key_bytes'])
    if measure(p,ARCHIVE_LIMIT)!={k:catalog['binary'][k] for k in ('sha256','bytes')}:raise ValueError('observed runtime changed during key read')
    if hashlib.sha256(der).hexdigest()!=catalog['key_sha256']:raise ValueError('observed verification key differs')
    rsa_key(der)
    return der

def project(delta_path,catalog_path,root=REPOSITORY):
    raw,_=consumed(delta_path,256 << 20);delta=strict_json(raw);catalog_raw,catalog_pin=consumed(catalog_path);catalog=strict_json(catalog_raw);key=observed_webstore_key(catalog)
    common=[dict(path=catalog_path.relative_to(root).as_posix(),**catalog_pin),dict(path=Path(__file__).relative_to(root).as_posix(),**measure(Path(__file__))),dict(path=(Path(__file__).with_name('project_archives.py')).relative_to(root).as_posix(),**measure(Path(__file__).with_name('project_archives.py')))]
    rows={row['path']:row for lane in delta['lanes'].values() for row in lane};result=dict(schema='airk.private.browser-extension-projection/1',files=[],errors=[],selected_count=0,signature_scope=catalog['scope'],input_sha256=hashlib.sha256(raw).hexdigest(),no_network=True,no_vendor_execution=True);cache={}
    for path,row in rows.items():
        match=re.fullmatch(r'(.*/Extensions/([a-p]{32})/([^/]+))/([^/]+)',path)
        if not match or match[4] not in NAMES:continue
        result['selected_count']+=1
        try:
            base,identifier,version,name=match.groups()
            if base not in cache:
                manifest_path=checked(root,base+'/manifest.json');proof_path=checked(root,base+'/_metadata/verified_contents.json')
                manifest_raw,manifest_pin=consumed(manifest_path);proof_raw,proof_pin=consumed(proof_path)
                manifest=strict_json(manifest_raw);proof=strict_json(proof_raw)
                if len(proof)!=1 or proof[0]['description']!='treehash per file':raise ValueError('unexpected signed manifest roster')
                publisher=base64.b64decode(manifest['key'],validate=True);rsa_key(publisher)
                derived=''.join(chr(ord('a')+int(c,16)) for c in hashlib.sha256(publisher).hexdigest()[:32])
                if derived!=identifier:raise ValueError('publisher key-derived extension ID differs')
                signed=proof[0]['signed_content'];signatures=signed['signatures'];publisher_sig=[s for s in signatures if s['header']['kid']=='publisher'];webstore_sig=[s for s in signatures if s['header']['kid']=='webstore']
                if publisher_sig:
                    if len(publisher_sig)!=1:raise ValueError('ambiguous publisher signature')
                    verify_signature(signed,publisher_sig[0],publisher);kind='publisher key-derived extension identity'
                elif len(webstore_sig)==1:verify_signature(signed,webstore_sig[0],key);kind='observed browser runtime webstore key'
                else:raise ValueError('no unique supported retained signature')
                payload=json.loads(decode(signed['payload']))
                if payload['item_id']!=identifier or payload['item_version']!=manifest['version'] or version!=manifest['version']+'_0' or payload['protocol_version']!=1:raise ValueError('signed extension identity/version differs')
                groups=payload['content_hashes']
                if len(groups)!=1 or groups[0]['format']!='treehash' or groups[0]['digest']!='sha256':raise ValueError('unsupported retained content hash scheme')
                group=groups[0];files={item['path']:item for item in group['files']}
                if len(files)!=len(group['files']):raise ValueError('duplicate signed file path')
                pins=[dict(path=manifest_path.relative_to(root).as_posix(),**manifest_pin),dict(path=proof_path.relative_to(root).as_posix(),**proof_pin)]
                cache[base]=(manifest,group,files,pins,kind)
            manifest,group,files,pins,kind=cache[base]
            source=checked(root,path);data,_=consumed(source)
            if len(data)>SOURCE_LIMIT or hashlib.sha256(data).hexdigest()!=row['source_sha256'] or len(data)!=row['bytes']:raise ValueError('source differs from frozen inventory')
            expected=decode(files[name]['root_hash'])
            if tree_hash(data,group['block_size'],group['hash_block_size'])!=expected:raise ValueError('file differs from signed manifest tree hash')
            result['files'].append(dict(source=path,source_sha256=row['source_sha256'],source_bytes=row['bytes'],disposition='upstream_build_input',representation='raw',reason='Source matches retained extension signed tree hash and key-derived identity; signature verified without browser execution. Observed runtime key scope is explicit, not a new publisher-download trust claim.',evidence=[*common,*pins],upstream=dict(repository='browser-extension/'+identifier,revision=manifest['version'],path=name,source_sha256=row['source_sha256']),projection=dict(signature_basis=kind,signed_tree_root=files[name]['root_hash'],block_size=4096,hash_block_size=4096)))
        except (OSError,ValueError,KeyError,TypeError) as error:result['errors'].append(dict(source=path,error=str(error)))
    try:
        for pin in [*common,*[pin for entry in cache.values() for pin in entry[3]]]:pinned(root,pin)
        if observed_webstore_key(catalog)!=key:raise ValueError('runtime key changed')
    except (OSError,ValueError,KeyError) as error:
        result['files']=[];result['errors'].append(dict(error=str(error),scope='final evidence validation'))
    result['verified_count']=len(result['files']);result['complete_within_selected_scope']=not result['errors'] and result['verified_count']==result['selected_count'];return result

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--delta',type=Path,required=True);p.add_argument('--catalog',type=Path,default=Path(__file__).with_name('browser-key-catalog.json'));a=p.parse_args();result=project(a.delta.resolve(),a.catalog.resolve());print(json.dumps(result,separators=(',',':')))
    if result['errors']:raise SystemExit(1)

if __name__=='__main__':main()

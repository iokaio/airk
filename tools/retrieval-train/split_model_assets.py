# SPDX-License-Identifier: Apache-2.0
"""Split an authenticated logical model asset into bounded private byte parts.

This creates payload bytes and developer evidence only. Native-owned downloads
metadata must be populated from the Rust-emitted template, not this receipt.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re

ROOT=Path(__file__).resolve().parents[2]


def split_asset(root,asset,output,prefix,part_size=1<<30):
    root=Path(root).resolve();output=Path(output).resolve();name=asset['name']
    if not re.fullmatch(r'[A-Za-z0-9_.-]{1,100}',prefix) or not 0<part_size<=1<<30:raise ValueError('unsafe prefix/part size')
    path=root/name
    if path.parent!=root or path.is_symlink() or not path.is_file() or path.stat().st_size!=asset['bytes']:raise ValueError('logical asset path/length differs')
    if not 0<asset['bytes']<=part_size*16:raise ValueError('logical asset part count exceeds sixteen')
    output.mkdir(exist_ok=False);parts=[];whole=hashlib.sha256();offset=0
    with path.open('rb') as stream:
        while offset<asset['bytes']:
            length=min(part_size,asset['bytes']-offset);target=output/(prefix+f'.part{len(parts)+1:03d}');observed=hashlib.sha256();remaining=length
            with target.open('xb') as sink:
                while remaining:
                    chunk=stream.read(min(4194304,remaining))
                    if not chunk:raise ValueError('logical asset truncated during split')
                    sink.write(chunk);whole.update(chunk);observed.update(chunk);remaining-=len(chunk)
            parts.append({'file':target.name,'offset':offset,'bytes':length,'sha256':observed.hexdigest()});offset+=length
        if stream.read(1):raise ValueError('logical asset grew during split')
    if whole.hexdigest()!=asset['sha256']:raise ValueError('copied logical bytes differ from authenticated asset')
    with path.open('rb') as stream:
        if hashlib.file_digest(stream,'sha256').hexdigest()!=asset['sha256']:raise ValueError('logical asset changed after splitting')
    return {'logical_name':name,'logical_bytes':offset,'logical_sha256':whole.hexdigest(),'parts':parts}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile',type=Path,required=True);parser.add_argument('--asset',required=True)
    parser.add_argument('--assets',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--prefix',required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):parser.error('fresh private output required')
    runner=Path(__file__).read_bytes();profile_bytes=args.profile.read_bytes();profile=json.loads(profile_bytes)
    matches=[asset for asset in profile['assets'] if asset['name']==args.asset]
    if len(matches)!=1:raise ValueError('logical asset absent/duplicated in native-owned profile instance')
    result=split_asset(args.assets,matches[0],output,args.prefix)
    if Path(__file__).read_bytes()!=runner or args.profile.read_bytes()!=profile_bytes:raise ValueError('part producer/profile drift')
    result.update(schema='airk.development-logical-asset-parts/1',profile_source_sha256=hashlib.sha256(profile_bytes).hexdigest(),runner_sha256=hashlib.sha256(runner).hexdigest(),scope='Private exact byte parts only; no native downloads DTO, hosting/upload, qualification or release claim')
    (output/'runner.py').write_bytes(runner);(output/'receipt.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'logical_name':result['logical_name'],'bytes':result['logical_bytes'],'parts':len(result['parts']),'sha256':result['logical_sha256']}))


if __name__=='__main__':main()

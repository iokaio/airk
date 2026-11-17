# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
r=Path.cwd();p=r/'artifacts/advanced-split/models/original-s8-native-qualification-r1';source=p/'native-proposed-r102.json'
assert hashlib.sha256(source.read_bytes()).hexdigest()=='e518fcd1472fb58c02109022d6a788243b574ca531ec063a4fb3d25a268e6036'
packet=json.loads(source.read_bytes());packet['status']='ROOT_RELEASED'
packet['root_review']='Root reviewed exact original S8 alone,12 same-graph oracle controls and serialB1, actual R102 Cargo/PE/source/owner/anchor evidence. R102 pure family refusal passed within167/167. Preserve original1800s/8GiB phase, original native owner and19176 runtime closure. Numeric/current-runtime evidence only; no task/resource-floor/platform/release qualification inferred.'
archive=p/'image-r102/airk-kernel-r102.exe';assert hashlib.sha256(archive.read_bytes()).hexdigest()==packet['binary']['sha256'];packet['binary']['path']=str(archive.resolve())
packet['pins'][str(archive.resolve())]=packet['binary']['sha256']
out=p/'native-released-root-r102.json';assert not out.exists();out.write_text(json.dumps(packet,indent=2)+'\n',encoding='utf-8');print(hashlib.sha256(out.read_bytes()).hexdigest())

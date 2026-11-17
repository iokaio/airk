# SPDX-License-Identifier: Apache-2.0
import hashlib,json,time
from pathlib import Path
root=Path(__file__).resolve().parents[4];out=Path(__file__).resolve().parent
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
descriptor=root/'artifacts/advanced-split/models/native-weight8-mixed-cpu8-cpu24-profiles-r1/inputs.json'
assert sha(descriptor)=='ae0f2bd9f19e374190cf0afdb2b86100f7d53933d0654801c1f812be5e616474'
historical=root/'artifacts/advanced-split/receipts/delivery-06-cpu24-numeric-parity-r1.json'
history={m['model']:m for m in json.loads(historical.read_text(encoding='utf-8'))['models']}
approved={'arbiter-small-intern-08b':'feff8b9d12f179ab9d719134bbedf2fe5d6a789d68761001130283ca082c9044','arbiter-small-strands-2b':'a4e7a8698709c8a5746cdea925d13579bbbec60c21223dfaccde93937c340b13','arbiter-large-intern-4b':'d10876d7b5b3da240fac17228985714f24aabbb6c53df35a8398b28fa04ed7bc','arbiter-large-kev-4b':'83e19e51906b2a7024af2432f6815ab6b032cd5945cf50ac5496f149c1a9f5e9'}
start=time.monotonic();models=[];pins={str(descriptor):sha(descriptor),str(historical):sha(historical)}
for entry in json.loads(descriptor.read_text(encoding='utf-8')):
    path=Path(entry['profile']);profile=json.loads(path.read_text(encoding='utf-8'));name=profile['model'];want=approved[name]
    assert sha(path)==want==history[name]['profile_source_sha256']
    assert profile['manifest']==history[name]['profile_manifest']
    pins[str(path)]=want
    assets=[];assetroot=Path(entry['assets']).resolve()
    for asset in profile['assets']:
        file=(assetroot/asset['name']).resolve();assert file.is_relative_to(assetroot)
        assert file.stat().st_size==asset['bytes'],file
        got=sha(file);assert got==asset['sha256'],(file,got)
        pins[str(file)]=got
        assets.append({'path':str(file),'native_asset':asset,'actual_sha256':got})
    assert sha(Path(entry['fixtures']))==history[name]['fixture_sha256']
    models.append({'model':name,'profile':str(path),'profile_source_sha256':want,'profile_manifest':profile['manifest'],'native_abi':profile['abi'],'assets':assets,'raw_only':True,'new_task_qualified':False})
    print(name,'sealed',sum(x['native_asset']['bytes'] for x in assets),'bytes',flush=True)
assert set(m['model'] for m in models)==set(approved)
receipt={'schema':'airk.private.context-action-raw-profile-seal/1','complete':True,'elapsed_seconds':time.monotonic()-start,'historical_authority':str(historical),'historical_authority_sha256':pins[str(historical)],'sha256_pins':pins,'models':models,'scope':'Actual byte SHA/length seal of all assets of four historical complete trained readouts and exact ABI/profile bytes. No model load, NN, calibration fit or new task admission.'}
(out/'asset-seal.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'receipt':str(out/'asset-seal.json'),'sha256':sha(out/'asset-seal.json'),'elapsed_seconds':receipt['elapsed_seconds'],'files':len(pins),'nn':False}),flush=True)

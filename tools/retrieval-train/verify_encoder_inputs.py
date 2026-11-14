# SPDX-License-Identifier: Apache-2.0
"""Independent masked-input fixtures matching live native B1 shape buckets."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]


def main():
    import numpy as np
    import onnxruntime as ort
    from tokenizers import Tokenizer
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exports',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--legacy-root',type=Path,help='Exact retained shipped v1 assets, explicitly evaluated on ORT1.30 only')
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):parser.error('fresh private output required')
    source=Path(__file__).read_bytes();receipt_raw=(args.exports/'receipt.json').read_bytes();receipt=json.loads(receipt_raw)
    pinned={args.exports/'fp32.onnx':receipt['fp32_sha256']}
    for profile in receipt['profiles']:
        directory=args.exports/('u8' if profile['precision'].endswith('u8') else 's8')
        pinned.update({directory/asset['name']:asset['sha256'] for asset in profile['assets']})
    def verify():
        for path,expected in pinned.items():
            with path.open('rb') as stream:observed=hashlib.file_digest(stream,'sha256').hexdigest()
            if observed!=expected:raise ValueError('encoder fixture asset changed: '+str(path))
    verify()
    tokenizer_bytes=(args.exports/'u8/tokenizer.json').read_bytes();tokenizer=Tokenizer.from_str(tokenizer_bytes.decode());tokenizer.no_padding();tokenizer.enable_truncation(max_length=256)
    options=ort.SessionOptions();options.intra_op_num_threads=1;options.inter_op_num_threads=1
    reference=ort.InferenceSession(str(args.exports/'fp32.onnx'),options,providers=['CPUExecutionProvider'])
    sessions={p['precision']:ort.InferenceSession(str(args.exports/('u8' if p['precision'].endswith('u8') else 's8')/'model.onnx'),options,providers=['CPUExecutionProvider']) for p in receipt['profiles']}
    legacy_metadata={}
    if args.legacy_root:
        catalog_path=ROOT/'crates/airk-retrieval/src/model_catalog_v1.json';catalog_bytes=catalog_path.read_bytes()
        snapshot=args.legacy_root/'compatibility-catalog-as-consumed.json'
        if snapshot.read_bytes()!=catalog_bytes:raise ValueError('original shipped compatibility catalog changed')
        legacy_metadata[catalog_path]=catalog_bytes;legacy_metadata[snapshot]=catalog_bytes
        catalog=json.loads(catalog_bytes)
        for short in ('u8','s8'):
            profile=next(p for p in catalog['profiles'] if p['id']=='minilm-l6-v2-'+short+'-v1');folder=args.legacy_root/short
            manifest_raw=(folder/'model.json').read_bytes();legacy_metadata[folder/'model.json']=manifest_raw
            if json.loads(manifest_raw)!=profile['manifest']:raise ValueError('original shipped manifest changed')
            pinned.update({folder/asset['name']:asset['sha256'] for asset in profile['assets']})
        verify()
        for short in ('u8','s8'):sessions['original-shipped-'+short+'-ort130-eval']=ort.InferenceSession(str(args.legacy_root/short/'model.onnx'),options,providers=['CPUExecutionProvider'])
    texts=[c['text'] for c in receipt['profiles'][0]['numerical_cases']]
    long='A permitted read preserves the source revision and original document location. '*60
    texts.extend([long,texts[0]])
    texts.extend(['Canonical evidence preserves source identity and caller scope. '*4,
                  'Canonical evidence preserves source identity and caller scope. '*8])
    cases=[]
    pool=lambda hidden,mask:((hidden*mask[:,:,None]).sum(1)/mask.sum(1,keepdims=True))[0]
    def normalized(hidden,mask):
        vector=pool(hidden,mask);return vector/np.linalg.norm(vector)
    for index,text in enumerate(texts):
        encoded=tokenizer.encode(text);length=next(bound for bound in (32,64,128,256) if bound>=len(encoded.ids));padding=length-len(encoded.ids)
        if index>=10 and length!=(64 if index==10 else 128):raise ValueError('independent bucket fixture does not exercise its declared size')
        feed={'input_ids':np.asarray([encoded.ids+[0]*padding],dtype=np.int64),'attention_mask':np.asarray([encoded.attention_mask+[0]*padding],dtype=np.int64),'token_type_ids':np.asarray([encoded.type_ids+[0]*padding],dtype=np.int64)}
        gold=normalized(reference.run(None,feed)[0],feed['attention_mask']);results={}
        for precision,session in sessions.items():
            vector=normalized(session.run(None,feed)[0],feed['attention_mask'])
            results[precision]={'vector':vector.tolist(),'cosine':float(vector@gold),'l2':float(np.linalg.norm(vector-gold)),'norm_error':float(abs(np.linalg.norm(vector)-1)),'finite':bool(np.isfinite(vector).all())}
        name='fixed-'+str(index) if index<8 else 'mixed-long' if index==8 else 'mixed-repeat-short' if index==9 else 'independent-bucket-64' if index==10 else 'independent-bucket-128'
        cases.append({'name':name,'text':text,'bucket':length,'real_tokens':len(encoded.ids),
            'inputs':{k:v.tolist() for k,v in feed.items()},'reference_vector':gold.tolist(),'results':results})
    for precision in sessions:
        if cases[0]['results'][precision]['vector']!=cases[9]['results'][precision]['vector']:raise ValueError('mixed-length caller order changed B1 result')
    if Path(__file__).read_bytes()!=source or (args.exports/'receipt.json').read_bytes()!=receipt_raw or (args.exports/'u8/tokenizer.json').read_bytes()!=tokenizer_bytes:raise ValueError('input fixture source drift')
    verify()
    if any(path.read_bytes()!=data for path,data in legacy_metadata.items()):raise ValueError('original baseline metadata changed')
    output.mkdir(parents=True)
    report={'schema':'airk.development-native-encoder-input-fixtures/1','source_sha256':hashlib.sha256(source).hexdigest(),'export_receipt_sha256':hashlib.sha256(receipt_raw).hexdigest(),'asset_seals':{str(p):value for p,value in pinned.items()},'baseline_metadata_seals':{str(p):hashlib.sha256(data).hexdigest() for p,data in legacy_metadata.items()},'original_untrained_fp32_reference':receipt.get('original_untrained_evaluation_baseline',False),'cases':cases,
        'caller_semantics':'Current onnx.rs tokenizes without padding, then pads EACH B1 tensor to32/64/128/256; mixed-length API batches execute those B1 calls serially. This is not a B>1 tensor qualification.',
        'scope':'Original developer numerical examples plus long masked input and repeated-short mixed-order check; no human final evaluation or native transport result'}
    (output/'runner.py').write_bytes(source);(output/'receipt.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({precision:{'min_cosine':min(c['results'][precision]['cosine'] for c in cases),'max_l2':max(c['results'][precision]['l2'] for c in cases),'max_norm_error':max(c['results'][precision]['norm_error'] for c in cases)} for precision in sessions}))


if __name__=='__main__':main()

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
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):parser.error('fresh private output required')
    source=Path(__file__).read_bytes();receipt_raw=(args.exports/'receipt.json').read_bytes();receipt=json.loads(receipt_raw)
    tokenizer_bytes=(args.exports/'u8/tokenizer.json').read_bytes();tokenizer=Tokenizer.from_str(tokenizer_bytes.decode());tokenizer.no_padding();tokenizer.enable_truncation(max_length=256)
    options=ort.SessionOptions();options.intra_op_num_threads=1;options.inter_op_num_threads=1
    reference=ort.InferenceSession(str(args.exports/'fp32.onnx'),options,providers=['CPUExecutionProvider'])
    sessions={p['precision']:ort.InferenceSession(str(args.exports/('u8' if p['precision'].endswith('u8') else 's8')/'model.onnx'),options,providers=['CPUExecutionProvider']) for p in receipt['profiles']}
    texts=[c['text'] for c in receipt['profiles'][0]['numerical_cases']]
    long='A permitted read preserves the source revision and original document location. '*60
    texts.extend([long,texts[0]])
    cases=[]
    pool=lambda hidden,mask:((hidden*mask[:,:,None]).sum(1)/mask.sum(1,keepdims=True))[0]
    def normalized(hidden,mask):
        vector=pool(hidden,mask);return vector/np.linalg.norm(vector)
    for index,text in enumerate(texts):
        encoded=tokenizer.encode(text);length=next(bound for bound in (32,64,128,256) if bound>=len(encoded.ids));padding=length-len(encoded.ids)
        feed={'input_ids':np.asarray([encoded.ids+[0]*padding],dtype=np.int64),'attention_mask':np.asarray([encoded.attention_mask+[0]*padding],dtype=np.int64),'token_type_ids':np.asarray([encoded.type_ids+[0]*padding],dtype=np.int64)}
        gold=normalized(reference.run(None,feed)[0],feed['attention_mask']);results={}
        for precision,session in sessions.items():
            vector=normalized(session.run(None,feed)[0],feed['attention_mask'])
            results[precision]={'vector':vector.tolist(),'cosine':float(vector@gold),'l2':float(np.linalg.norm(vector-gold)),'norm_error':float(abs(np.linalg.norm(vector)-1)),'finite':bool(np.isfinite(vector).all())}
        cases.append({'name':'fixed-'+str(index) if index<8 else 'mixed-long' if index==8 else 'mixed-repeat-short','text':text,'bucket':length,'real_tokens':len(encoded.ids),
            'inputs':{k:v.tolist() for k,v in feed.items()},'reference_vector':gold.tolist(),'results':results})
    for precision in sessions:
        if cases[0]['results'][precision]['vector']!=cases[-1]['results'][precision]['vector']:raise ValueError('mixed-length caller order changed B1 result')
    if Path(__file__).read_bytes()!=source or (args.exports/'receipt.json').read_bytes()!=receipt_raw or (args.exports/'u8/tokenizer.json').read_bytes()!=tokenizer_bytes:raise ValueError('input fixture source drift')
    output.mkdir(parents=True)
    report={'schema':'airk.development-native-encoder-input-fixtures/1','source_sha256':hashlib.sha256(source).hexdigest(),'export_receipt_sha256':hashlib.sha256(receipt_raw).hexdigest(),'cases':cases,
        'caller_semantics':'Current onnx.rs tokenizes without padding, then pads EACH B1 tensor to32/64/128/256; mixed-length API batches execute those B1 calls serially. This is not a B>1 tensor qualification.',
        'scope':'Original developer numerical examples plus long masked input and repeated-short mixed-order check; no human final evaluation or native transport result'}
    (output/'runner.py').write_bytes(source);(output/'receipt.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({precision:{'min_cosine':min(c['results'][precision]['cosine'] for c in cases),'max_l2':max(c['results'][precision]['l2'] for c in cases),'max_norm_error':max(c['results'][precision]['norm_error'] for c in cases)} for precision in sessions}))


if __name__=='__main__':main()

# SPDX-License-Identifier: Apache-2.0
"""Export the actual trained MiniLM encoder and both explicit INT8 candidates."""
import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path
import time

ROOT=Path(__file__).resolve().parents[2]
from safe_int8 import recode_unsigned_weights


def sha(raw):return hashlib.sha256(raw).hexdigest()


def main():
    import numpy as np
    import onnx
    import onnxruntime as ort
    from onnxruntime.quantization import QuantType,quantize_dynamic
    from safetensors.torch import load
    import torch
    from tokenizers import Tokenizer
    from transformers import BertConfig,BertModel
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--checkpoint',type=Path,required=True)
    parser.add_argument('--corpus',type=Path,required=True)
    parser.add_argument('--documentation',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--baseline-original',action='store_true',help='Explicit evaluation-only original weights with the exact same mixed quantization recipe')
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):parser.error('fresh private output required')
    files=(Path(__file__),Path(__file__).with_name('safe_int8.py'),args.checkpoint/'model.safetensors',args.checkpoint/'config.json',args.checkpoint/'tokenizer.json',args.corpus,args.documentation,ROOT/'target/local-vector-model/minilm-l6-v2-u8-v1/LICENSE')
    retained={p:p.read_bytes() for p in files}
    if sha(retained[args.corpus])!='6e9d8ade6f799ed2a5e1903de69482db9fcd3cb1731a5fee4a504adede5fca6f' or sha(retained[args.documentation])!='916b398de0fc9774041fe174d0e585c862bc68e9aa18fa04028c3126f693a18f':raise ValueError('unreviewed training provenance')
    model=BertModel(BertConfig.from_dict(json.loads(retained[args.checkpoint/'config.json'])),add_pooling_layer=False)
    state=load(retained[args.checkpoint/'model.safetensors']);excluded=[]
    if args.baseline_original:
        expected={'model.safetensors':'53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db','config.json':'953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41','tokenizer.json':'be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037'}
        if any(sha(retained[args.checkpoint/name])!=value for name,value in expected.items()):raise ValueError('original baseline source differs from approved exact publisher assets')
        excluded=sorted(set(state)-set(model.state_dict()))
        if not set(excluded)<={'pooler.dense.weight','pooler.dense.bias','embeddings.position_ids'}:raise ValueError('unreviewed original encoder state exclusion')
        state={key:value for key,value in state.items() if key not in excluded}
    model.load_state_dict(state,strict=True);model.eval();torch.set_num_threads(2)
    class Encoder(torch.nn.Module):
        def __init__(self,inner):super().__init__();self.inner=inner
        def forward(self,input_ids,attention_mask,token_type_ids):return self.inner(input_ids=input_ids,attention_mask=attention_mask,token_type_ids=token_type_ids).last_hidden_state
    wrapper=Encoder(model).eval();output.mkdir(parents=True);started=time.monotonic()
    inputs=(torch.ones(1,16,dtype=torch.long),torch.ones(1,16,dtype=torch.long),torch.zeros(1,16,dtype=torch.long))
    torch.onnx.export(wrapper,inputs,str(output/'fp32.onnx'),input_names=['input_ids','attention_mask','token_type_ids'],output_names=['last_hidden_state'],
        dynamic_axes={name:{1:'tokens'} for name in ('input_ids','attention_mask','token_type_ids','last_hidden_state')},opset_version=18,dynamo=False)
    corpus=json.loads(retained[args.corpus]);documentation=json.loads(retained[args.documentation]);train_groups={t['source_group'] for t in corpus['tasks'] if t['split']=='train'}
    notices=['\n\nAIRK TRAINING MODIFICATIONS AND DATA ATTRIBUTION\n',
        'These encoder weights were changed by local contrastive question-to-evidence training. Base: sentence-transformers/all-MiniLM-L6-v2 at10dbd2f06a8baf40ad285b037edda610c1b9a57c, Apache-2.0. The tokenizer is unchanged. Actual training/source hashes and recipe are retained in the release qualification receipt.\n',
        'QASPER: Pradeep Dasigi, Kyle Lo, Iz Beltagy, Arman Cohan, Noah A. Smith and Matt Gardner (2021), A Dataset of Information-Seeking Questions and Answers Anchored in Research Papers, CC BY4.0 https://creativecommons.org/licenses/by/4.0/ . Original human questions/evidence and normalized S2ORC text used; quote-bearing passages and figures/tables excluded.\n']
    for decision in corpus['qasper_rights_decisions']:
        if 'qasper:'+decision['paper_id'] in train_groups:notices.append(decision['attribution']+' '+decision['url']+' '+decision['license_url']+'\n')
    notices.append('Additional mechanical documentation/function training pairs: Requests2.25.1, psf/requests commitc2b307dbefe21177af03f9feb37181a89a799fcc, Apache-2.0. Original developer docstrings and containing function source retained; instruction indentation normalized and first documentation paragraph selected. No independent human context-sufficiency label is asserted.\n')
    for artifact in documentation['license_artifacts']:notices.append('\nRequests '+artifact['file']+'\n'+artifact['text']+'\n')
    license_bytes=retained[files[-1]]+''.join(notices).encode()
    if args.baseline_original:
        license_bytes=retained[files[-1]]+b'\n\nAIRK EVALUATION-ONLY ORIGINAL ENCODER BASELINE\nExact original sentence-transformers/all-MiniLM-L6-v2 weights at10dbd2f06a8baf40ad285b037edda610c1b9a57c, Apache-2.0. No AIRK training/data modifications. Converted using the same mixed quantization recipe as the trained candidate to separate training and precision effects. Original mean/L2 embedding pipeline never consumes the unused publisher pooler; all required encoder tensors retained. No production v1 execution, promotion or native quality claim.\n'
    tokenizer=Tokenizer.from_str(retained[args.checkpoint/'tokenizer.json'].decode());tokenizer.no_padding();tokenizer.enable_truncation(max_length=256)
    texts=['A local pipe serves same-user clients.','HTTPS serves browser clients.','The cache expires after30seconds.',
        'The Ravens team is based in Riverton.','A permitted read preserves the indexed revision.',
        'The result excludes unrelated source roots.','Café 日本語 🚀.','Negation: this evidence does not justify the claim.']
    vectors=[];feeds=[]
    with torch.inference_mode():
        for text in texts:
            encoded=tokenizer.encode(text);feed={'input_ids':np.asarray([encoded.ids],dtype=np.int64),'attention_mask':np.asarray([encoded.attention_mask],dtype=np.int64),'token_type_ids':np.asarray([encoded.type_ids],dtype=np.int64)}
            hidden=wrapper(*(torch.from_numpy(feed[k]) for k in ('input_ids','attention_mask','token_type_ids'))).numpy()
            vector=(hidden*feed['attention_mask'][:,:,None]).sum(1)/feed['attention_mask'].sum(1,keepdims=True);vector/=np.linalg.norm(vector,axis=1,keepdims=True)
            feeds.append(feed);vectors.append(vector[0])
    profiles=[]
    float_graph=onnx.load(str(output/'fp32.onnx'))
    protected=['/inner/encoder/layer.'+str(layer)+'/output/dense/MatMul' for layer in range(6)]
    actual_nodes={node.name for node in float_graph.graph.node}
    if not set(protected)<=actual_nodes:raise ValueError('sensitive FFN output node inventory changed')
    initializer_elements={tensor.name:int(np.prod(tensor.dims)) for tensor in float_graph.graph.initializer}
    constant_linear={node.name:initializer_elements[node.input[1]] for node in float_graph.graph.node if node.op_type=='MatMul' and node.input[1] in initializer_elements}
    for short,weight_type in [('u8',QuantType.QUInt8),('s8',QuantType.QInt8)]:
        profile_protected=protected
        if not set(profile_protected)<=actual_nodes:raise ValueError('protected residual output node inventory changed')
        folder=output/short;folder.mkdir()
        if short=='s8':
            signed_graph=onnx.load(str(output/'u8/model.onnx'))
            signed_lowering=recode_unsigned_weights(signed_graph)
            onnx.save_model(signed_graph,str(folder/'model.onnx'))
        else:quantize_dynamic(str(output/'fp32.onnx'),str(folder/'model.onnx'),weight_type=weight_type,per_channel=True,nodes_to_exclude=profile_protected,op_types_to_quantize=['MatMul'],extra_options={'MatMulConstBOnly':True})
        (folder/'tokenizer.json').write_bytes(retained[args.checkpoint/'tokenizer.json']);(folder/'LICENSE').write_bytes(license_bytes)
        options=ort.SessionOptions();options.intra_op_num_threads=1;options.inter_op_num_threads=1
        session=ort.InferenceSession(str(folder/'model.onnx'),options,providers=['CPUExecutionProvider']);cases=[]
        for text,feed,reference in zip(texts,feeds,vectors):
            hidden=session.run(['last_hidden_state'],feed)[0];vector=(hidden*feed['attention_mask'][:,:,None]).sum(1)/feed['attention_mask'].sum(1,keepdims=True);vector/=np.linalg.norm(vector,axis=1,keepdims=True);vector=vector[0]
            cases.append({'text':text,'inputs':{k:v.tolist() for k,v in feed.items()},'reference_vector':reference.tolist(),'actual_vector':vector.tolist(),'cosine':float(vector @ reference),'l2':float(np.linalg.norm(vector-reference)),'norm_error':float(abs(np.linalg.norm(vector)-1)),'finite':bool(np.isfinite(vector).all())})
        manifest={'schema':2,'name':'sentence-transformers/all-MiniLM-L6-v2','license':'Apache-2.0','source':'https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2','revision':'10dbd2f06a8baf40ad285b037edda610c1b9a57c',
            'model_sha256':sha((folder/'model.onnx').read_bytes()),'tokenizer_sha256':sha((folder/'tokenizer.json').read_bytes()),'license_sha256':sha(license_bytes),'dimensions':384,'max_tokens':256,'pooling':'attention-mask-mean-l2','input_ids':'input_ids','attention_mask':'attention_mask','token_type_ids':'token_type_ids','output':'last_hidden_state'}
        (folder/'model.json').write_text(json.dumps(manifest,indent=2)+'\n')
        profiles.append({'id':'minilm-l6-v2-'+short+('-original-mixed-eval-v1' if args.baseline_original else '-v2'),'precision':'mixed-'+short,'signed_reduce_range':False,'activation_precision':'unsigned asymmetric8bit split into exact low7/high1 MatMulInteger passes' if short=='s8' else 'unsigned asymmetric INT8','signed_lowering':signed_lowering if short=='s8' else None,'fp32_embeddings_retained':True,'protected_fp32_nodes':profile_protected,
            'constant_linear_parameters':sum(constant_linear.values()),'quantized_linear_parameters':sum(v for k,v in constant_linear.items() if k not in profile_protected),
            'manifest':manifest,'assets':[{'name':p.name,'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in (folder/'model.onnx',folder/'tokenizer.json',folder/'LICENSE')],'numerical_cases':cases})
    if any(path.read_bytes()!=raw for path,raw in retained.items()):raise ValueError('encoder export input drift')
    report={'schema':'airk.development-trained-encoder-exports/1','original_untrained_evaluation_baseline':args.baseline_original,'excluded_unused_publisher_state_keys':excluded,'source_model_sha256':sha(retained[args.checkpoint/'model.safetensors']),'fp32_sha256':sha((output/'fp32.onnx').read_bytes()),'profiles':profiles,
        'input_seals':{str(p):sha(raw) for p,raw in retained.items()},'elapsed_seconds':time.monotonic()-started,'versions':{n:importlib.metadata.version(n) for n in ('torch','onnx','onnxruntime','transformers')},
        'scope':'Exact original untrained encoder converted using the same explicit mixed quantization recipe as the trained candidate; no local training or production v1 execution claim. Evaluation-only baseline, not native quality/platform/package qualification.' if args.baseline_original else 'Actual selected trained encoder and two explicit INT8 exports. Base revision identifies ancestry, not unmodified upstream output bytes; LICENSE records training modifications/data attribution. V2profileproposal only, not catalog/default/index identity migration or quality/native/platform/package qualification.'}
    (output/'runner.py').write_bytes(retained[Path(__file__)]);(output/'safe_int8.py').write_bytes(retained[Path(__file__).with_name('safe_int8.py')]);(output/'receipt.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'profiles':[{'id':p['id'],'model_sha256':p['manifest']['model_sha256'],'min_cosine':min(c['cosine'] for c in p['numerical_cases']),'max_l2':max(c['l2'] for c in p['numerical_cases'])} for p in profiles],'elapsed_seconds':report['elapsed_seconds']}),flush=True)


if __name__=='__main__':main()

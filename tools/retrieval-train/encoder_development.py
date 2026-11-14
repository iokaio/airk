# SPDX-License-Identifier: Apache-2.0
"""Select a trained encoder using development tasks only, before final scoring."""
import argparse
import hashlib
import json
from pathlib import Path
import time

ROOT=Path(__file__).resolve().parents[2]


def sha(data):return hashlib.sha256(data).hexdigest()


def coverage(task,hits):
    alternatives=[a for a in task['human_alternatives'] if not a['unanswerable'] and a['alignment']=='all_original_evidence_maps_to_allowed_text']
    if not alternatives:return None
    scores=[]
    for alternative in alternatives:
        evidence=alternative['evidence']
        scores.append(sum(bool(set(e['allowed_locations']) & hits) for e in evidence)/len(evidence))
    return max(scores),int(any(value==1 for value in scores))


def main():
    import numpy as np
    import torch
    from safetensors.torch import load
    from tokenizers import Tokenizer
    from transformers import BertConfig,BertModel
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--corpus',type=Path,required=True)
    parser.add_argument('--source',type=Path,required=True)
    parser.add_argument('--trained',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):parser.error('fresh private output required')
    retained={p:p.read_bytes() for p in (Path(__file__),args.corpus,args.source/'tokenizer.json')}
    corpus=json.loads(retained[args.corpus]);train_development_groups={t['source_group'] for t in corpus['tasks'] if t['split']!='test' and t['dataset']=='QASPER'}
    documents=sorted([d for d in corpus['documents'] if d['dataset']=='QASPER' and d['source_group'] in train_development_groups],key=lambda d:d['id'])
    tasks=sorted([t for t in corpus['tasks'] if t['split']=='development' and coverage(t,set()) is not None],key=lambda t:t['id'])
    if not tasks:raise ValueError('no independent human development questions')
    tokenizer=Tokenizer.from_str(retained[args.source/'tokenizer.json'].decode());tokenizer.enable_truncation(max_length=256);tokenizer.enable_padding(pad_id=0,pad_token='[PAD]')
    device='cuda';torch.set_num_threads(2);torch.backends.cuda.matmul.allow_tf32=False
    output.mkdir(parents=True);reports=[];started=time.monotonic()
    for epoch in range(4):
        folder=args.source if epoch==0 else args.trained/('epoch-'+str(epoch))
        config_bytes=(folder/'config.json').read_bytes();weights=(folder/'model.safetensors').read_bytes()
        model=BertModel(BertConfig.from_dict(json.loads(config_bytes)),add_pooling_layer=False)
        state={k:v for k,v in load(weights).items() if not k.startswith('pooler.') and k!='embeddings.position_ids'}
        model.load_state_dict(state,strict=True);model.to(device).eval()
        def encode(texts):
            rows=[]
            with torch.inference_mode():
                for start in range(0,len(texts),32):
                    encoded=tokenizer.encode_batch(texts[start:start+32]);masks=torch.tensor([e.attention_mask for e in encoded],device=device)
                    hidden=model(input_ids=torch.tensor([e.ids for e in encoded],device=device),attention_mask=masks,
                        token_type_ids=torch.tensor([e.type_ids for e in encoded],device=device)).last_hidden_state
                    pooled=(hidden*masks.unsqueeze(-1)).sum(1)/masks.sum(1,keepdim=True).clamp_min(1)
                    rows.append(torch.nn.functional.normalize(pooled.float(),dim=1).cpu().numpy())
            return np.concatenate(rows)
        vectors=encode([d['text'] for d in documents]);queries=encode([t['question'] for t in tasks]);scores=queries @ vectors.T
        results=[]
        for task,row in zip(tasks,scores):
            permitted=[i for i,d in enumerate(documents) if d['source_group']==task['source_group']]
            order=sorted(permitted,key=lambda i:(-float(row[i]),documents[i]['id']))[:10]
            hits={documents[i]['id'] for i in order};recall,complete=coverage(task,hits)
            results.append({'task_id':task['id'],'recall':recall,'complete':complete,'hits':[{'id':documents[i]['id'],'score':float(row[i])} for i in order]})
        report={'epoch':epoch,'model_sha256':sha(weights),'config_sha256':sha(config_bytes),'selected_tasks':len(tasks),
            'complete_original_alternative_recall':sum(r['complete'] for r in results)/len(results),'mean_original_alternative_evidence_recall':sum(r['recall'] for r in results)/len(results),'results':results}
        reports.append(report);print(json.dumps({k:v for k,v in report.items() if k!='results'}),flush=True)
        if (folder/'model.safetensors').read_bytes()!=weights or (folder/'config.json').read_bytes()!=config_bytes:raise ValueError('encoder drift')
        del model,state,weights;torch.cuda.empty_cache()
    chosen=max(reports[1:],key=lambda r:(r['complete_original_alternative_recall'],r['mean_original_alternative_evidence_recall'],-r['epoch']))
    if any(p.read_bytes()!=raw for p,raw in retained.items()):raise ValueError('development input drift')
    report={'schema':'airk.development-encoder-selection/1','source_sha256':sha(retained[Path(__file__)]),'corpus_sha256':sha(retained[args.corpus]),
        'candidate_documents':len(documents),'reports':reports,'chosen_epoch':chosen['epoch'],
        'selection':'Highest development complete-original-alternative recall, then mean evidence recall, then earliest epoch; baseline never chosen because actual trained export required; no quality pass inferred',
        'elapsed_seconds':time.monotonic()-started,'scope':'Development-only9humanquestions; original publisher-paper request scope, identical across baseline/candidates, before reading answers/evidence. No final/Hotpot task scoring or final quality claim. Candidate corpus excludes all final source groups.'}
    (output/'receipt.json').write_text(json.dumps(report,indent=2)+'\n');(output/'runner.py').write_bytes(retained[Path(__file__)])
    print(json.dumps({'chosen_epoch':chosen['epoch'],'elapsed_seconds':report['elapsed_seconds']}))


if __name__=='__main__':main()

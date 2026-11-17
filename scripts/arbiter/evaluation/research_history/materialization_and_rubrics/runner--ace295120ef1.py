# SPDX-License-Identifier: Apache-2.0
"""Create private exact-text native indexing fixtures and freeze goal task IDs."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--corpus',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):parser.error('fresh private output required')
    raw=args.corpus.read_bytes();source=Path(__file__).read_bytes();freeze=(Path(__file__).parent/'a0-freeze.json').read_bytes()
    sha=lambda data:hashlib.sha256(data).hexdigest()
    if sha(raw)!='6e9d8ade6f799ed2a5e1903de69482db9fcd3cb1731a5fee4a504adede5fca6f':raise ValueError('unreviewed corpus')
    corpus=json.loads(raw);output.mkdir(parents=True);mapping=[]
    for document in corpus['documents']:
        body=document['text'].encode()
        if sha(body)!=document['paragraph_sha256']:raise ValueError('source paragraph identity differs')
        family='hotpot' if document['dataset']=='HotpotQA' else 'qasper/'+document['source_group'].split(':',1)[1]
        path=output/'sources'/family/(sha(document['id'].encode())+'.txt');path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(body)
        mapping.append({'id':document['id'],'source_group':document['source_group'],'file':path.relative_to(output).as_posix(),
            'sha256':sha(body),'bytes':len(body),'title':document['title'],'original_url':document['url'],'license':document['license']})
    eligible=[t for t in corpus['tasks'] if t['split']=='test' and any(a['alignment']=='all_original_evidence_maps_to_allowed_text' and not a['unanswerable'] for a in t['human_alternatives'])]
    chosen=sorted((t['id'] for t in eligible),key=lambda identifier:sha(('airk.goal.final.v1:'+identifier).encode()))[:100]
    if len(chosen)!=100:raise ValueError('frozen goal population unavailable')
    if args.corpus.read_bytes()!=raw or Path(__file__).read_bytes()!=source or (Path(__file__).parent/'a0-freeze.json').read_bytes()!=freeze:raise ValueError('fixture input drift')
    report={'schema':'airk.private-native-human-fixtures/1','corpus_sha256':sha(raw),'freeze_sha256':sha(freeze),'runner_sha256':sha(source),
        'files':mapping,'goal_ids':chosen,'goal_selection':'First100 SHA256 domain-separated eligible final task IDs; selected before any final model scoring; no resampling',
        'native_scopes':{'HotpotQA':'sources/hotpot (GLOBAL complete cross-document collection)','QASPER':'sources/qasper/<original publisher paper ID> (original paper-bound request scope)'},
        'rights':'Private licensed fixtures only. Preserve per-source attribution and rights in corpus; Hotpot-derived text is CC BY-SA4 evaluation-only, never distributable-weight training. Exact paragraph bytes retained without added headers.',
        'scope':'Fixture materialization and pre-score goal-ID freeze only, no native indexing/transport/model quality result'}
    (output/'manifest.private.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
    (output/'a0-freeze-as-consumed.json').write_bytes(freeze);(output/'runner.py').write_bytes(source)
    print(json.dumps({'source_files':len(mapping),'goal_tasks':len(chosen),'manifest_sha256':sha((output/'manifest.private.json').read_bytes())}))


if __name__=='__main__':main()

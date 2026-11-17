# SPDX-License-Identifier: Apache-2.0
"""Classify pinned JSON indicator fields without copying or executing their text.

Prints metadata only. Every rule inspects decoded structure/content or an exact
maintained excerpt; filenames alone never authorize an exclusion.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from extract import pointer

ROOT=Path(__file__).resolve().parents[4]
BASE=Path(__file__).resolve().parent

def sha(raw):return hashlib.sha256(raw).hexdigest()

def proof(name,note):
    raw=(ROOT/name).read_bytes()
    return {'path':name,'sha256':sha(raw),'bytes':len(raw),'reason':note}

def verdict(kind,reason,evidence=None):
    return {'disposition':kind,'reason':reason,'evidence':evidence or []}

def _classify_bound(doc,ptr,text):
    key=ptr.rsplit('/',1)[-1]
    if isinstance(doc,dict):
        schema=doc.get('schema')
        if schema in {'airk.relevance-corpus/1','airk.p13a.additional-original-labels/1'} and ptr.startswith('/sources/') and isinstance(doc.get('queries'),list):
            return verdict('private_retrieval_fixture_input','Source string belongs to explicitly labelled relevance corpus sources/queries; fixture examples are inputs, not implementation.',[proof('scripts/fixtures/retrieval-v190.json','Maintained relevance fixture schema/source-query roles')])
        if schema=='airk.private-original-documentation-pairs/1' and doc.get('revision')=='c2b307dbefe21177af03f9feb37181a89a799fcc' and ('/documents/' in ptr or '/tasks/' in ptr):
            if not all(x.get('url','').startswith('https://github.com/psf/requests/blob/'+doc['revision']+'/') for x in doc['documents']):raise ValueError('Unexpected Requests source authority')
            return verdict('private_upstream_training_input','Original Requests function/docstring pairs are declared train-only private dataset inputs, not AIRK implementation.',[proof('scripts/arbiter/evaluation/research_history/materialization_and_rubrics/requests_pairs--7dfa7c6dd0d1.py','AST extraction from pinned Requests archive and private pair producer')])
        if {'version','page','rows'}<=doc.keys() and ptr.startswith('/rows/') and ('/hit/' in ptr or '/fact/' in ptr) and key in {'target','signature','name','qualified_name'}:
            if not all(isinstance(row,dict) and ('hit' in row or 'fact' in row) for row in doc['rows']):raise ValueError('Unexpected native fact page')
            return verdict('private_native_index_projection','version/page/rows hit fields are indexed symbol/signature facts, not authored executable source.',[proof('crates/airk-store/src/current/native_projection.rs','NativeFactPage/NativeFactRow owner and current-resources persistence')])
        if ptr.startswith('/indexes_before/') and key=='source_text' and {'fixture_root','source_inputs','candidate'}<=doc.keys():
            return verdict('private_upgrade_fixture_input','Upgrade receipt retains pre-upgrade source_text to prove input preservation.',[proof('scripts/check-runtime-upgrade.py','Fixture source_text capture and preservation comparisons')])
        if doc.get('fixture_kind') and 'fixture_script_sha256' in doc and (ptr.startswith('/workbench/') or ptr.startswith('/read_comparisons/')):
            return verdict('private_native_read_comparison','Parity receipt actual/expected native result or copied/exported representation; fixture producer identity is recorded in container.',[proof('scripts/arbiter/research/operational_history_r2/sources/3cb7a3ceb29a3c0ba2f9b10ebb279f76efe0d5bd2f8565a9b44fdcb207116391--check-p6f-scoped-parity.py','Retained scoped parity producer records actual/expected/copy/export result fields')])
        if schema=='airk-golden-cli/2' and ptr.startswith('/cases/') and key=='stdout' and {'program','platform'}<=doc.keys():
            return verdict('cli_golden_output','CLI golden stdout documentation/output case, not executable implementation.')
        if key in {'usage','arbiter'} and ('Usage:' in text or 'usage:' in text or 'airk ' in text):
            return verdict('help_documentation','Selected usage/arbiter field is human-readable CLI/help documentation with command examples.',[proof('crates/airk-mcp/src/help/content.json','Canonical native help content owner')])
        if 'pip_version' in doc and 'install' in doc and ptr.startswith('/install/') and key=='description':
            return verdict('third_party_package_description','pip resolver install metadata description string, not authored AIRK source.')
        if {'info','urls','last_serial'}<=doc.keys() and key=='details' and '/vulnerabilities/' in ptr:
            return verdict('upstream_security_advisory','PyPI package metadata vulnerability details; code syntax is advisory text.')
        if doc.get('package')=='@vscode/proxy-agent' and doc.get('version')=='0.45.0' and key=='snippet' and ptr.startswith('/snippets/') and len(doc.get('member_sha256',''))==64:
            return verdict('third_party_package_excerpt','Audit captures excerpts of @vscode/proxy-agent/out/index.js with archive/member hashes; third-party package research input.')
        if {'id','content','title','text','metadata'}<=doc.keys() and ptr=='/text':
            return verdict('private_publisher_document','Publisher document content/text alongside title/metadata; retained as labelled research input, not AIRK implementation.')
        if {'original_600_TRAIN_pin','original_archive_lfs_sha256','documents','tasks'}<=doc.keys() and key=='original_human_answer':
            return verdict('private_publisher_training_answer','Human answer under source-manifest tasks; original TRAIN/archive pins are retained in container.')
        if {'label','replies'}<=doc.keys() and ptr.startswith('/replies/') and '/result/hits/' in ptr and key=='text':
            return verdict('private_native_read_result','Combined read response structuredContent result hit text, not implementation source.')
        if {'calls','metadata'}<=doc.keys() and ptr.startswith('/calls/') and ('/result/' in ptr or '/parsed/' in ptr):
            return verdict('private_native_read_result','Recorded MCP call result field containing indexed text or symbol names.')
        if 'checks' in doc and ptr.startswith('/checks/') and '/data/' in ptr and key in {'text','name'}:
            return verdict('private_native_read_result','Manual MCP check data hit text or symbol-name result, not an implementation field.')
        if 'turns' in doc and ptr.startswith('/turns/') and '/envelope/' in ptr and key=='text':
            return verdict('private_workbench_read_or_answer','Exported Workbench turn envelope paragraphs or canonical result hit text.',[proof('scripts/check-workbench-window-scope.py','Synthetic native read/answer fixture producer')])
        if key in {'Description','Message'} and 'WorkItem' in doc and ('Comments' in doc or 'Description' in doc['WorkItem']):
            return verdict('upstream_issue_discussion_pending_proof','Archived WorkItem description/comment is issue research text; exact upstream membership proof remains pending.')
        if key=='text' and (ptr.startswith('/observations/') or ptr.startswith('/workbench/observations/')):
            return verdict('private_workbench_read_or_answer','Workbench observation native turn envelope/read/answer presentation text.',[proof('scripts/check-workbench-window-scope.py','Synthetic fixture creation and workbench observation collection')])
        if key=='text' and (('/hits/' in ptr) or ('/evidence/' in ptr) or ('/paragraphs/' in ptr)) and ('search' in doc or 'observations' in doc or 'answer' in doc or 'envelope' in doc or 'workbench' in doc or 'messages' in doc):
            return verdict('private_native_read_or_answer','Structured native result/answer hit/evidence/paragraph text, not an implementation field.',[proof('scripts/check-workbench-window-scope.py','Synthetic native read/answer fixture producer')])
        if key in {'message','description','modelDescription'} and (doc.get('name') or ptr.startswith('/configuration.')):
            return verdict('third_party_extension_documentation','Extension manifest/localization documentation field, not authored AIRK implementation.')
        if key=='description' and ('$schema' in doc or 'definitions' in doc or '$defs' in doc):
            return verdict('json_schema_documentation','JSON-schema description string contains documentation/example syntax.')
        if key in {'Description','Message','Html'} and any(k in doc for k in ('Id','IssueId','DiscussionId','WorkItemId','id','Title','title')):
            return verdict('upstream_issue_discussion','Issue/discussion body inside third-party archive; requires independent upstream container proof before closure.')
    if isinstance(doc,list):
        if key=='Html' and all(isinstance(x,dict) and 'Html' in x for x in doc):
            return verdict('upstream_issue_discussion_pending_proof','Archived discussion HTML entries; exact upstream membership proof remains pending.')
        if all(isinstance(x,dict) and {'id','path','text','line','tokens'}<=x.keys() for x in doc) and key=='text':
            return verdict('private_indexed_corpus_input','Corpus entries carry original path/line/token metadata and copied text; indexed source copies remain private.')
        if all(isinstance(x,dict) and {'QUESTION_ID','QUESTION_TEXT','ANSWER','DOC_IDS'}<=x.keys() for x in doc) and key in {'ANSWER','QUESTION_TEXT'}:
            return verdict('private_publisher_training_answer','Original human question/answer records with document identifiers; dataset input, not AIRK implementation.')
    return verdict('unresolved','No structural/producer proof rule matched; retain for explicit review.')

def classify(doc,ptr,text,*,container=None,raw=None):
    # Authentication of the finite reviewed scope precedes all structural rules.
    # Generic keys such as name/description or observations never grant eligibility.
    if container is None or raw is None:
        return verdict('unresolved','No authenticated reviewed container was supplied.')
    scope=json.loads((BASE/'classification-scope.json').read_bytes())
    pins={row['container']:row for row in scope['files']}
    pin=pins.get(container)
    if pin is None or sha(raw)!=pin['sha256'] or len(raw)!=pin['bytes']:
        return verdict('unresolved','Container is outside the exact reviewed path/byte scope.')
    actual=json.loads(raw)
    if actual!=doc or pointer(actual,'$'+ptr)!=text:
        raise ValueError('Caller substituted the reviewed document or pointer text')
    return _classify_bound(actual,ptr,text)

def excerpt_proof(text):
    payload=text.encode()
    paths=subprocess.run(['git','ls-files','-z','*.rs'],cwd=ROOT,capture_output=True,check=True).stdout.decode().split('\0')
    for name in paths:
        path=ROOT/name
        if not name or not path.is_file():continue
        raw=path.read_bytes()
        for normalization,body in [('none',raw),('CRLF-to-LF-only',raw.replace(b'\r\n',b'\n'))]:
            offset=body.find(payload)
            if offset>=0:return verdict('maintained_source_excerpt','Exact diagnostic excerpt is already contained in current maintained Rust source; declared newline comparison only.',[dict(proof(name,'Current source excerpt owner'),normalization=normalization,byte_offset=offset,excerpt_bytes=len(payload))])
    name='crates/airk-cli/src/service_management_receipt_tests.rs'
    revision='a490471688c0ef5813414667299ba6f239e6cf9b'
    raw=subprocess.run(['git','show',revision+':'+name],cwd=ROOT,capture_output=True,check=True).stdout
    if payload in raw:return verdict('git_retained_diagnostic_excerpt','Exact diagnostic excerpt survives in reachable historical Rust source; current implementation has moved.',[{'revision':revision,'path':name,'sha256':sha(raw),'bytes':len(raw),'byte_offset':raw.index(payload)}])
    return verdict('unresolved','Source excerpt has no exact maintained or reviewed Git owner.')

def main(path):
    raw=path.read_bytes();remainder=json.loads(raw);covered={};maps=[]
    for p in (BASE/'supplemental/source-provenance.json',BASE/'supplemental/encoded-source-provenance.json'):
        doc=json.loads(p.read_bytes());maps.append(proof(p.relative_to(ROOT).as_posix(),'Reviewed direct/encoded source preservation'))
        for row in doc['files']:covered[(row['container'],row['json_pointer'])]=row
    output=[];cache={}
    for row in remainder['remaining_indicator_fields']:
        if not row['container'].endswith('.json'):continue
        name=row['container'];ptr=row['json_pointer_or_text_root']
        if name not in cache:
            source=(ROOT/name).read_bytes()
            if sha(source)!=row['container_sha256'] or len(source)!=row['container_bytes']:raise ValueError('Container drift: '+name)
            cache[name]=(json.loads(source),source)
        doc,source=cache[name];text=pointer(doc,'$'+ptr)
        if not isinstance(text,str) or sha(text.encode())!=row['text_sha256']:raise ValueError('Field drift: '+name+ptr)
        result=verdict('maintained_authored_source','Exact decoded source preserved by supplemental map.',maps) if (name,ptr) in covered else classify(doc,ptr,text,container=name,raw=source)
        if result['disposition']=='unresolved' and (ptr.endswith('/source_context') or (ptr.startswith('/files/') and '/change/' in ptr)):
            result=excerpt_proof(text)
        output.append(dict(row,**result))
    print(json.dumps({'schema':'airk.embedded-indicator-disposition/1','input':{'path':path.relative_to(ROOT).as_posix(),'sha256':sha(raw)},'classifier':proof(Path(__file__).relative_to(ROOT).as_posix(),'Exact classifier implementation'),'reviewed_scope':proof((BASE/'classification-scope.json').relative_to(ROOT).as_posix(),'Exact reviewed container eligibility'),'files':output,'complete_source_coverage':False,'scope':'Only the1486remaining JSON indicator fields; no other scanner gaps or1718document/textcontainers are closed.'},indent=2))

if __name__=='__main__':
    main(Path(sys.argv[1]).resolve())

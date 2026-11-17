# SPDX-License-Identifier: Apache-2.0
import ast,hashlib,io,json,tarfile,textwrap
from pathlib import Path
root=Path(__file__).parent/'requests-train-rights';raw=(root/'source.tgz').read_bytes();commit=json.loads((root/'commit.json').read_text())['sha'];assert commit=='c2b307dbefe21177af03f9feb37181a89a799fcc'
documents=[];tasks=[];licenses=[];sources=[]
with tarfile.open(fileobj=io.BytesIO(raw),mode='r:gz') as archive:
 members=archive.getmembers();assert len(members)<1000
 for member in members:
  if not member.isfile():continue
  relative='/'.join(Path(member.name).parts[1:])
  if relative in ('LICENSE','NOTICE'):
   data=archive.extractfile(member).read();(root/relative).write_bytes(data);licenses.append({'file':relative,'sha256':hashlib.sha256(data).hexdigest(),'text':data.decode()})
  if not relative.endswith('.py') or member.size>1048576:continue
  data=archive.extractfile(member).read();text=data.decode('utf-8-sig');sources.append({'path':relative,'sha256':hashlib.sha256(data).hexdigest()})
  try:tree=ast.parse(text)
  except SyntaxError:continue
  for node in ast.walk(tree):
   if not isinstance(node,(ast.FunctionDef,ast.AsyncFunctionDef)):continue
   doc=ast.get_docstring(node,clean=True)
   if not doc:continue
   instruction=doc.split('\n\n')[0].strip()
   if not 20<=len(instruction)<=1500:continue
   body=ast.get_source_segment(text,node)
   if not body or len(body)>16384:continue
   identifier='requests:'+relative+':'+str(node.lineno)+':'+node.name
   documents.append({'id':identifier,'text':body,'title':relative+' '+node.name,'source_group':'github:psf/requests@'+commit,
    'url':'https://github.com/psf/requests/blob/'+commit+'/'+relative+'#L'+str(node.lineno),'license':'Apache-2.0','dataset':'requests-original-docstrings','paragraph_sha256':hashlib.sha256(body.encode()).hexdigest(),'source_path':relative,'source_file_sha256':hashlib.sha256(data).hexdigest(),
    'source_line':node.lineno,'annotation_authority':'Published pre2021 original developer docstring, mechanically paired with containing original function; no independently adjudicated retrieval/sufficiency label'})
   tasks.append({'id':identifier,'question_id':identifier,'question':instruction,'dataset':'requests-original-docstrings','split':'train','publisher_split':'unpartitioned source, entire project family train-only','source_group':'github:psf/requests@'+commit,
    'disagreement':False,'human_alternatives':[{'human_annotation_id':identifier+':published-docstring','unanswerable':False,'original_evidence_count':1,'evidence':[{'source_evidence_sha256':hashlib.sha256(body.encode()).hexdigest(),'allowed_locations':[identifier]}],'alignment':'all_original_evidence_maps_to_allowed_text','original_human_answer':{'documentation':doc}}]})
assert licenses and 'Apache License' in licenses[0]['text'] and 'Version 2.0' in licenses[0]['text']
result={'schema':'airk.private-original-documentation-pairs/1','documents':documents,'tasks':tasks,'license_artifacts':licenses,'source_files':sources,'archive_sha256':hashlib.sha256(raw).hexdigest(),'revision':commit,
 'rights_scope':'TRAIN ONLY, exact historical Apache-2.0 source; preserve original copyright/license/notice. Source code and human-published developer docstrings retained privately. No Hotpot/distributable sharealike fixture content.',
 'annotation_limit':'Mechanical containing-function associations are not independent human retrieval/context-role/sufficiency gold. No model-authored query or answer labels. Original code contains its own docstring, so lexical shortcut/generalization limits must be disclosed.',
 'changes':'Original function body retained exactly, AST-provided original line span; human docstring cleaned of indentation and first paragraph selected as instruction; stable source identity and licensing metadata added.'}
output=root/'pairs.private.json';assert not output.exists();output.write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({'documents':len(documents),'tasks':len(tasks),'source_files':len(sources),'sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'archive_sha256':result['archive_sha256'],'license_files':[x['file'] for x in licenses]}))

# SPDX-License-Identifier: Apache-2.0
import json,pathlib,queue,subprocess,threading,time,hashlib,tarfile
ASSETS=pathlib.Path(__file__).resolve().parent
REPORT=json.loads((ASSETS/'campaign.json').read_text())
def write_json(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def digest(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def inventory(root):
 return [{'path':str(p.relative_to(root)),'bytes':p.stat().st_size,'sha256':digest(p)} for p in sorted(root.rglob('*')) if p.is_file()]
def stdio(rec,argv,cwd):
 log=cwd/'observations';transcript=[];messages=queue.Queue()
 with (log/'mcp-saved-read.stderr').open('wb') as err:
  child=subprocess.Popen(argv,cwd=cwd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
  def reader():
   for line in child.stdout:
    if len(line)>1024*1024:messages.put(RuntimeError('MCP line exceeds 1MiB'));return
    try:messages.put(json.loads(line))
    except Exception as ex:messages.put(ex);return
  thread=threading.Thread(target=reader,daemon=True);thread.start()
  try:
   def exchange(message):
    transcript.append({'sent':message});child.stdin.write((json.dumps(message)+'\n').encode());child.stdin.flush()
    if 'id' not in message:return
    deadline=time.monotonic()+30
    while True:
     reply=messages.get(timeout=max(0.001,deadline-time.monotonic()))
     if isinstance(reply,Exception):raise reply
     transcript.append({'received':reply})
     if reply.get('id')==message['id']:
      assert 'error' not in reply,reply
      return reply
   initialize=exchange({'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2025-06-18','capabilities':{},'clientInfo':{'name':'airk-historical-fixture','version':'1'}}})
   exchange({'jsonrpc':'2.0','method':'notifications/initialized'})
   response=exchange({'jsonrpc':'2.0','id':2,'method':'tools/call','params':{'name':'airk_status','arguments':{}}})
   assert not response['result'].get('isError'),response
   rec['mcp_read_status']=response
   for ident,name,args in [(3,'airk_read_file',{'path':'README.md','start_line':1,'end_line':2}),(4,'airk_search',{'query':'cobalt','mode':'lexical'})]:
    result=exchange({'jsonrpc':'2.0','id':ident,'method':'tools/call','params':{'name':name,'arguments':args}})
    assert not result['result'].get('isError'),result
    assert 'cobalt' in json.dumps(result).lower(),result
    rec[name]=result
   rec['negotiated_protocol']=initialize['result']['protocolVersion']
  finally:
   child.stdin.close()
   try:code=child.wait(timeout=10)
   except subprocess.TimeoutExpired:
    child.terminate()
    try:code=child.wait(timeout=5)
    except subprocess.TimeoutExpired:child.kill();code=child.wait(timeout=5)
   thread.join(timeout=2)
   rec['mcp_saved_read_process']={'pid':child.pid,'exit_code':code,'owned_child_reaped':True,'reader_joined':not thread.is_alive()}
   write_json(log/'mcp-saved-read-transcript.json',transcript)

for rec in REPORT['candidates']:
 assert rec['outcome']=='created'
 case=pathlib.Path(rec['source']).parent
 before=inventory(pathlib.Path(rec['data_root']))
 stdio(rec,rec['original_config']['actual_stdio_argv'],case)
 assert json.loads(rec['mcp_read_status']['result']['content'][0]['text'])['publication']==rec['status']['publication']
 after=inventory(pathlib.Path(rec['data_root']))
 old={f['path']:f for f in before};new={f['path']:f for f in after}
 differences=[{'path':name,'before':old.get(name),'after':new.get(name)} for name in sorted(set(old)|set(new)) if old.get(name)!=new.get(name)]
 rec['saved_read_native_metadata_differences']=differences
 rec['saved_data_after_content_reads']=after
 assert all(d['path'].replace(chr(92),'/').endswith('/usage.sqlite3') for d in differences),'unexpected original evidence changed'
 rec['saved_read_evidence_files_unchanged']=True
 archive=case/'saved-index-after-read.tar'
 with tarfile.open(archive,'x') as tar:
  for name in ['private-data','workspace','User/config.toml']:tar.add(case/name,arcname=name,recursive=True)
 rec['saved_index_after_read_archive']={'path':str(archive),'sha256':digest(archive),'bytes':archive.stat().st_size}
 for f in rec['source_after']:assert digest(pathlib.Path(rec['source'])/f['path'])==f['sha256']
 rec['saved_read_data_unchanged']=not differences
 write_json(case/'receipt.json',rec)
 print(json.dumps({'tag':rec['tag'],'saved_read':'passed','child':rec['mcp_saved_read_process']}),flush=True)
write_json(pathlib.Path(REPORT['base'])/'campaign.json',REPORT)
write_json(ASSETS/'campaign.json',REPORT)

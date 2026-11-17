# SPDX-License-Identifier: Apache-2.0
"""GPU-owner COFF/Ninja inspection. This performs no build or library loading."""
from pathlib import Path,PurePosixPath
import re,struct,hashlib
MAX_ARCHIVE=2*1024**3
MAX_MEMBER=512*1024**2
BIGOBJ=bytes.fromhex('c7a1bad1eebaa94baf20faf66aa4dcb8')
PROVIDER_IMPORTS={'onnxruntime_providers_shared.lib':'onnxruntime_providers_shared.dll','onnxruntime_providers_cuda.lib':'onnxruntime_providers_cuda.dll'}

def relative(name):
 if not isinstance(name,str) or not name or any(c.isspace() or ord(c)<32 for c in name) or any(c in name for c in '$\\:') or name.startswith('/') or any(x in ('','.','..') for x in name.split('/')):raise ValueError('unsafe relative Ninja artifact')
 return PurePosixPath(name)

def _member(stream,size):
 start=stream.tell();head=stream.read(min(size,56))
 if len(head)<20:raise ValueError('short COFF archive member')
 sections=0;offset=0;imports=[];exports=[]
 if head[:4]==b'\x00\x00\xff\xff':
  version,machine=struct.unpack_from('<HH',head,4)
  if machine!=0x8664:raise ValueError('non-x64 COFF member')
  if version<2:
   payload=struct.unpack_from('<I',head,12)[0]
   if payload>65536 or 20+payload>size:raise ValueError('invalid short import record size')
   stream.seek(start+20);parts=stream.read(payload).split(b'\x00')
   if len(parts)<3:raise ValueError('short import record lacks DLL identity')
   dll=parts[1].decode('ascii',errors='strict')
   if not re.fullmatch(r'[A-Za-z0-9_.-]+\.dll',dll,re.I):raise ValueError('unsafe import DLL identity')
   return 'import',[dll.lower()],[]
  if version!=2 or len(head)<56 or head[12:28]!=BIGOBJ:raise ValueError('unknown anonymous/LTO COFF member')
  sections=struct.unpack_from('<I',head,44)[0];offset=56
 else:
  machine,sections=struct.unpack_from('<HH',head,0)
  if machine!=0x8664:raise ValueError('non-x64/non-COFF archive member')
  optional=struct.unpack_from('<H',head,16)[0]
  if optional!=0:raise ValueError('archive member contains an image optional header')
  offset=20
 if sections>100000 or offset+40*sections>size:raise ValueError('COFF section table escapes member')
 stream.seek(start+offset)
 for _ in range(sections):
  section=stream.read(40)
  if len(section)!=40:raise ValueError('short COFF section')
  name=section[:8].rstrip(b'\x00')
  if name.startswith(b'.idata'):return 'import_descriptor',[],[]
  if name==b'.drectve':
   length,pointer=struct.unpack_from('<II',section,16)
   if length>1024**2 or pointer+length>size:raise ValueError('COFF linker directives exceed member')
   saved=stream.tell();stream.seek(start+pointer);directives=stream.read(length).decode('ascii',errors='strict');stream.seek(saved)
   exports+=re.findall(r'[/-]EXPORT:([A-Za-z_][A-Za-z0-9_@?]*)',directives,re.I)
 return 'static',[],exports

def inspect_archive(path):
 path=Path(path);size=path.stat().st_size
 if not 8<size<=MAX_ARCHIVE:raise ValueError('archive size out of bounds')
 members=0;static=0;imports=0;dlls=set();exports=set();longnames=None;bodies={}
 with path.open('rb') as stream:
  if stream.read(8)!=b'!<arch>\n':raise ValueError('not a COFF library archive')
  while stream.tell()<size:
   header=stream.read(60)
   if len(header)!=60 or header[58:]!=b'`\n':raise ValueError('malformed archive member header')
   raw=header[:16].decode('ascii',errors='strict').rstrip();field=header[48:58].decode('ascii',errors='strict').strip()
   if not field.isdecimal():raise ValueError('invalid archive member length')
   length=int(field);start=stream.tell();end=start+length
   if end>size or length>MAX_MEMBER:raise ValueError('archive member exceeds file/bound')
   if raw=='//':
    if length>16*1024**2:raise ValueError('oversized archive name table')
    longnames=stream.read(length)
   elif raw not in ('/','/SYM64/'):
    if raw.startswith('/'):
     if not raw[1:].isdecimal() or longnames is None or int(raw[1:])>=len(longnames):raise ValueError('invalid long archive member name')
    members+=1
    if members>200000:raise ValueError('archive member count exceeds bound')
    kind,names,exported=_member(stream,length);dlls.update(names);exports.update(exported)
    stream.seek(start);h=hashlib.sha256();remaining=length
    while remaining:
     block=stream.read(min(1024*1024,remaining))
     if not block:raise ValueError('short archive object body')
     h.update(block);remaining-=len(block)
    key=(h.hexdigest(),length);bodies[key]=bodies.get(key,0)+1
    if kind=='static':static+=1
    else:imports+=1
   stream.seek(end)
   if length%2 and stream.read(1)!=b'\n':raise ValueError('invalid archive padding')
  if stream.tell()!=size or members==0:raise ValueError('empty/truncated archive')
 return {'kind':'static' if imports==0 else 'import_or_mixed','members':members,'static_members':static,'import_members':imports,'imported_dlls':sorted(dlls),'export_directives':sorted(exports),'object_bodies':[{'sha256':h,'bytes':n,'count':count} for (h,n),count in sorted(bodies.items())]}

def runtime_closure(text,roots):
 if len(text.encode('utf8'))>64*1024**2:raise ValueError('Ninja graph exceeds bound')
 static={};modules={};imports={}
 for line in text.replace('$\n','').splitlines():
  if not line.startswith('build '):continue
  match=re.fullmatch(r'build ([^:]+): ([^ ]+)(?: (.*))?',line)
  if not match:continue
  outs=[x for x in match[1].split() if x!='|'];rule=match[2];tail=match[3] or ''
  if rule.startswith(('CXX_STATIC_LIBRARY_LINKER','C_STATIC_LIBRARY_LINKER')):
   if len(outs)!=1:raise ValueError('ambiguous static output')
   name=relative(outs[0].replace(chr(92),'/')).as_posix()
   if not name.endswith('.lib') or name in static:raise ValueError('duplicate/foreign static output')
   dependencies=[];order_only=False
   for token in tail.split():
    if token=='||':order_only=True;continue
    if token=='|' or not token.endswith('.lib'):continue
    dependencies.append((relative(token.replace(chr(92),'/')).as_posix(),order_only))
   static[name]=dependencies
  elif rule.startswith(('CXX_SHARED_LIBRARY_LINKER','CXX_MODULE_LIBRARY_LINKER','C_SHARED_LIBRARY_LINKER','C_MODULE_LIBRARY_LINKER','CUDA_MODULE_LIBRARY_LINKER','CUDA_SHARED_LIBRARY_LINKER')):
   libraries=[relative(x.replace(chr(92),'/')).as_posix() for x in outs if x.endswith('.lib')];dlls=[relative(x.replace(chr(92),'/')).as_posix() for x in outs if x.endswith('.dll')]
   if libraries or dlls:
    if len(libraries)>1 or len(dlls)!=1:raise ValueError('ambiguous shared import output')
    module=dlls[0];name=libraries[0] if libraries else None
    if module in modules or name is not None and (name in imports or name in static):raise ValueError('duplicate static/shared output')
    if PurePosixPath(module).name.lower()=='onnxruntime.dll':raise ValueError('second dynamic ORT core refused')
    modules[module]={'import_library':name,'link_rule':rule}
    if name is not None:imports[name]=module
 selected=set();visiting=set();excluded={}
 def visit(name):
  relative(name)
  if name in visiting:raise ValueError('cyclic static target dependency')
  if name in selected:return
  if name not in static:raise ValueError('missing genuine static dependency: '+name)
  visiting.add(name)
  for child,order_only in static[name]:
   if child in imports:
    module=imports[child];expected=PROVIDER_IMPORTS.get(child)
    if not order_only or expected!=module:raise ValueError('unexpected direct/dynamic import dependency in core')
    excluded[child]=module
   else:visit(child)
  visiting.remove(name);selected.add(name)
 for name in roots:visit(name)
 if not selected or len(selected)>256:raise ValueError('static closure count out of bounds')
 for name,module in PROVIDER_IMPORTS.items():
  if module not in modules:raise ValueError('same-build provider pair missing from Ninja graph')
  actual=modules[module]['import_library']
  if actual is not None and actual!=name:raise ValueError('provider import library identity differs')
  if module=='onnxruntime_providers_shared.dll' and actual is None:raise ValueError('shared provider interface has no import library')
 return {'static_inputs':sorted(selected),'excluded_ordered_provider_imports':[{'path':k,'module':v} for k,v in sorted(excluded.items())],'provider_modules':{k:modules[k] for k in sorted(PROVIDER_IMPORTS.values())}}

def verify_member_closure(inputs,output):
 expected={}
 for archive in inputs:
  if archive['kind']!='static':raise ValueError('nonstatic aggregate input')
  for row in archive['object_bodies']:
   key=(row['sha256'],row['bytes']);expected[key]=expected.get(key,0)+row['count']
 actual={(r['sha256'],r['bytes']):r['count'] for r in output['object_bodies']}
 if output['kind']!='static' or set(actual)!=set(expected) or any(not 1<=count<=expected[key] for key,count in actual.items()):raise ValueError('aggregate object byte closure differs from genuine inputs')

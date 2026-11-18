# SPDX-License-Identifier: Apache-2.0
"""Graph-only prepared provenance and retained read-only artifact custody."""
import ctypes
import datetime
import copy
from ctypes import wintypes as w
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time
import types

ROOT = Path(__file__).resolve().parents[2]
SID = 'S-1-5-21-3678370870-3858642417-826968105-1002'
SCHEMA = 'airk.private.cuda-graph-provenance-preparation/1'
MAX_FILES = 512
MAX_JSON = 16 << 20
TRUSTED_OWNERS = {SID, 'S-1-5-18', 'S-1-5-32-544',
    'S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464'}

def require(ok, message):
    if not ok:
        raise ValueError(message)

def check(deadline):
    require(type(deadline) in (int,float) and math.isfinite(deadline) and time.monotonic() < deadline,
            'original graph provenance deadline expired')

def key(path):
    return os.path.normcase(str(Path(path)))

def strict_json(data, maximum=MAX_JSON):
    require(len(data) <= maximum, 'graph provenance JSON cap')
    def pairs(rows):
        result = {}
        for name,value in rows:
            require(name not in result, 'duplicate graph provenance JSON key')
            result[name] = value
        return result
    def invalid(value):
        raise ValueError('nonfinite graph provenance JSON')
    return json.loads(data,object_pairs_hook=pairs,parse_constant=invalid)

def canonical(value):
    return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':'),
        ensure_ascii=False,allow_nan=False).encode()).hexdigest()

def file_pin(row):
    require(isinstance(row,dict) and set(row) == {'path','bytes','sha256'}
        and isinstance(row['path'],str) and Path(row['path']).is_absolute()
        and type(row['bytes']) is int and 0 <= row['bytes'] < 1 << 63
        and isinstance(row['sha256'],str) and len(row['sha256']) == 64
        and all(c in '0123456789abcdef' for c in row['sha256']), 'exact boundary FilePin required')
    return row

class WindowsFile:
    """Noninheritable handle; read sharing only prevents writes and deletion."""
    def __init__(self, path):
        require(os.name == 'nt', 'retained graph custody requires Windows')
        import msvcrt
        self.path = Path(path)
        require(self.path.is_absolute() and self.path.resolve(strict=True) == self.path,
                'canonical graph boundary path required')
        for p in [self.path,*self.path.parents]:
            require(not p.is_symlink() and not getattr(p.lstat(),'st_file_attributes',0) & 0x400,
                    'graph boundary reparse path refused')
        self.api = ctypes.WinDLL('kernel32',use_last_error=True)
        api = self.api
        api.CreateFileW.argtypes = [w.LPCWSTR,w.DWORD,w.DWORD,ctypes.c_void_p,w.DWORD,w.DWORD,w.HANDLE]
        api.CreateFileW.restype = w.HANDLE
        api.CloseHandle.argtypes = [w.HANDLE]
        api.CloseHandle.restype = w.BOOL
        api.GetFileInformationByHandleEx.argtypes = [w.HANDLE,ctypes.c_int,ctypes.c_void_p,w.DWORD]
        api.GetFileInformationByHandleEx.restype = w.BOOL
        api.GetFinalPathNameByHandleW.argtypes = [w.HANDLE,w.LPWSTR,w.DWORD,w.DWORD]
        api.GetFinalPathNameByHandleW.restype = w.DWORD
        handle = api.CreateFileW(str(self.path),0x80000000,1,None,3,0x00200000|0x08000000,None)
        require(handle not in (None,ctypes.c_void_p(-1).value), 'boundary read-only open failed: '+str(self.path))
        fd = None
        try:
            self.handle = handle
            observed = self.identity()
            require(observed['links'] == 1 and observed['attributes'] & (0x400|0x10) == 0,
                    'boundary link/directory/reparse object refused')
            text = ctypes.create_unicode_buffer(32768)
            n = api.GetFinalPathNameByHandleW(handle,text,len(text),0)
            require(0 < n < len(text), 'boundary final handle path unavailable')
            actual = text.value
            if actual.startswith('\\\\?\\'):
                actual = actual[4:]
            require(key(actual) == key(self.path), 'boundary handle resolved a different path')
            fd = msvcrt.open_osfhandle(int(handle),os.O_RDONLY|os.O_BINARY)
            self.stream = os.fdopen(fd,'rb',buffering=0)
        except BaseException:
            if fd is None:
                api.CloseHandle(handle)
            else:
                os.close(fd)
            raise

    def identity(self):
        class FileId(ctypes.Structure):
            _fields_ = [('volume',ctypes.c_ulonglong),('id',ctypes.c_ubyte*16)]
        class Standard(ctypes.Structure):
            _fields_ = [('allocation',ctypes.c_longlong),('size',ctypes.c_longlong),
                        ('links',w.DWORD),('pending',ctypes.c_ubyte),('directory',ctypes.c_ubyte)]
        class Basic(ctypes.Structure):
            _fields_ = [('created',ctypes.c_longlong),('accessed',ctypes.c_longlong),
                        ('written',ctypes.c_longlong),('changed',ctypes.c_longlong),('attributes',w.DWORD)]
        values = []
        for kind,cls in [(18,FileId),(1,Standard),(0,Basic)]:
            value = cls()
            if not self.api.GetFileInformationByHandleEx(self.handle,kind,ctypes.byref(value),ctypes.sizeof(value)):
                raise ctypes.WinError(ctypes.get_last_error())
            values.append(value)
        fid,standard,basic = values
        return dict(volume_serial=int(fid.volume),file_id=bytes(fid.id).hex(),links=int(standard.links),
                    bytes=int(standard.size),last_write=int(basic.written),attributes=int(basic.attributes),
                    delete_pending=bool(standard.pending),directory=bool(standard.directory))

    def read(self, size):
        return self.stream.read(size)

    def rewind(self):
        self.stream.seek(0)

    def seek(self, offset):
        self.stream.seek(offset)

    def close(self):
        self.stream.close()

def policies(plan, paths, deadline):
    check(deadline)
    budget = min(10.0,deadline-time.monotonic())
    require(budget > 0, 'graph custody policy deadline expired')
    code = ("$ErrorActionPreference='Stop'; $v=[Console]::In.ReadToEnd()|ConvertFrom-Json; "
        "$sw=[Diagnostics.Stopwatch]::StartNew(); . $v.guard; "
        "$identity=Assert-AirkHumanExecution -ExpectedSid $v.sid; "
        "$rows=@(foreach($p in $v.paths){if($sw.Elapsed.TotalSeconds -ge $v.budget){throw 'policy deadline'}; "
        "$a=Get-Acl -LiteralPath $p; @{path=$p;owner_sid=$a.GetOwner([Security.Principal.SecurityIdentifier]).Value;sddl=$a.Sddl}}); "
        "@{identity=$identity;rows=$rows}|ConvertTo-Json -Depth 8 -Compress")
    payload = json.dumps(dict(guard=plan['checked_guard']['path'],sid=SID,paths=list(paths),budget=budget))
    require(len(payload.encode()) <= 2 << 20, 'graph policy request cap')
    observed = subprocess.run([plan['powershell']['path'],'-NoLogo','-NoProfile','-NonInteractive','-Command',code],
        input=payload,capture_output=True,text=True,check=True,timeout=budget)
    check(deadline)
    value = strict_json(observed.stdout.encode())
    identity = value['identity']
    require(identity['UserSid'] == identity['OwnerSid'] == SID and SID in identity['InteractiveOwnerSids'],
            'graph policy identity differs')
    require(len(value['rows']) == len(paths), 'graph policy roster incomplete')
    result = {}
    for path,row in zip(paths,value['rows']):
        require(row['path'] == path and row['owner_sid'] in TRUSTED_OWNERS and isinstance(row['sddl'],str)
                and row['sddl'], 'graph boundary owner/policy refused')
        result[key(path)] = dict(owner_sid=row['owner_sid'],sddl=row['sddl'])
    return dict(identity=identity,policies=result)

class Lease:
    def __init__(self, plan, pins, deadline, *, expected=None, opener=WindowsFile, observe=policies):
        require(0 < len(pins) <= MAX_FILES, 'graph boundary file count')
        self.plan,self.deadline,self.observe = plan,deadline,observe
        self.files,self.rows,self.loaded,self.modules,self.previous = {},{},{},{},{}
        self.closed = False
        self.close_errors = []
        self.closed_count = 0
        try:
            for pin in pins:
                file_pin(pin);k=key(pin['path'])
                require(k not in self.files, 'duplicate graph boundary path')
                check(deadline)
                opened = opener(pin['path'])
                self.files[k] = opened
                identity = opened.identity()
                require(identity['links'] == 1 and not identity['delete_pending'] and not identity['directory']
                        and identity['attributes'] & 0x400 == 0, 'unprotected boundary object')
                actual = self.hash(k)
                require(actual == pin, 'opened graph boundary bytes differ: '+pin['path'])
                require(opened.identity() == identity, 'boundary identity changed during acquisition')
                self.rows[k] = dict(pin=pin,identity=identity)
            observed = observe(plan,[r['pin']['path'] for r in self.rows.values()],deadline)
            self.identity = observed['identity']
            require(self.identity['UserSid']==self.identity['OwnerSid']==SID
                    and SID in self.identity['InteractiveOwnerSids'],'initial identity differs')
            require(set(observed['policies'])==set(self.rows),'initial policy roster differs')
            for k,row in self.rows.items():
                row['policy'] = observed['policies'][k]
                if expected is not None and k in expected:
                    require(row == expected[k], 'prepared graph object identity/policy/bytes changed')
            check(deadline)
        except BaseException as error:
            try:
                self.close()
            except BaseException as close_error:
                error.add_note('Boundary close failed: '+str(close_error))
            raise

    def hash(self, k):
        check(self.deadline)
        opened = self.files[k]
        opened.rewind()
        digest,total = hashlib.sha256(),0
        while True:
            check(self.deadline)
            data = opened.read(4 << 20)
            check(self.deadline)
            if not data:
                break
            digest.update(data);total += len(data)
        path = self.rows[k]['pin']['path'] if k in self.rows else str(opened.path)
        return dict(path=path,bytes=total,sha256=digest.hexdigest())

    def bytes(self, path, maximum=MAX_JSON):
        k=key(path);require(k in self.files and not self.closed,'unheld boundary read')
        require(self.rows[k]['pin']['bytes'] <= maximum,'bounded boundary read required')
        check(self.deadline)
        opened=self.files[k];opened.rewind();parts=[];size=0
        while True:
            check(self.deadline);data=opened.read(min(4<<20,maximum+1-size));check(self.deadline)
            if not data:break
            parts.append(data);size+=len(data);require(size<=maximum,'boundary read cap')
        data=b''.join(parts)
        require(len(data)==self.rows[k]['pin']['bytes'] and hashlib.sha256(data).hexdigest()==self.rows[k]['pin']['sha256'],'held boundary changed')
        return data

    def json(self,path,maximum=MAX_JSON):
        return strict_json(self.bytes(path,maximum),maximum)

    def pin(self,path,**kwargs):
        check(self.deadline)
        require(key(path) in self.rows and not self.closed,'pin outside retained boundary')
        return dict(self.rows[key(path)]['pin'])

    def digest(self,path):
        check(self.deadline)
        require(key(path) in self.rows and not self.closed,'digest outside exact retained boundary')
        # These bytes were freshly hashed at acquisition and cannot be written or
        # deleted through another handle. refresh() performs final actual hashing.
        return self.rows[key(path)]['pin']['sha256']

    def refresh(self):
        require(not self.closed,'closed boundary cannot be refreshed')
        for k,row in self.rows.items():
            check(self.deadline)
            require(self.files[k].identity()==row['identity'] and self.hash(k)==row['pin']
                    and self.files[k].identity()==row['identity'],
                    'retained graph boundary changed')
        observed=self.observe(self.plan,[r['pin']['path'] for r in self.rows.values()],self.deadline)
        require(observed['identity']['UserSid']==observed['identity']['OwnerSid']==SID
                and SID in observed['identity']['InteractiveOwnerSids'],'final identity differs')
        require(observed['policies']=={k:r['policy'] for k,r in self.rows.items()},'final boundary ACL drift')
        check(self.deadline)

    def load(self,path,name):
        k=key(path);require(k in self.rows and Path(path).suffix=='.py','unheld executable source refused')
        require(Path(path).is_relative_to(ROOT/'scripts'),'artifact executable source refused')
        if k in self.modules:
            module=self.modules[k]
        else:
            raw=self.bytes(path,2<<20)
            module=types.ModuleType(name);module.__file__=str(Path(path));module.__package__=''
            self.previous.setdefault(name,sys.modules.get(name))
            sys.modules[name]=module
            self.modules[k]=module
            exec(compile(raw,str(path),'exec',dont_inherit=True),module.__dict__)
            module.__airk_loaded_source__=dict(self.rows[k]['pin'])
            self.loaded[k]=dict(self.rows[k]['pin']);self.modules[k]=module
            # Private module bindings only. Selected execution modules and
            # ownership-worker imports are covered by the exact held roster.
            if callable(getattr(module,'load',None)):module.load=self.load
            for attr in ('module','_load'):
                if callable(getattr(module,attr,None)):
                    setattr(module,attr,lambda leaf,_p=Path(path).parent:self.load(_p/(leaf+'.py'),leaf.replace('-','_')))
        self.previous.setdefault(name,sys.modules.get(name))
        sys.modules[name]=module
        return module

    def close(self):
        if self.closed:return
        self.closed=True
        for opened in reversed(list(self.files.values())):
            try:
                opened.close()
                self.closed_count += 1
            except BaseException as error:self.close_errors.append(type(error).__name__+': '+str(error))
        for name,old in self.previous.items():
            current=sys.modules.get(name)
            if current in self.modules.values():
                if old is None:sys.modules.pop(name,None)
                else:sys.modules[name]=old
        require(not self.close_errors,'boundary handle close failed: '+repr(self.close_errors))

    def records(self):
        return [self.rows[k] for k in sorted(self.rows)]

    def __enter__(self):return self
    def __exit__(self,kind,error,trace):
        try:self.close()
        except BaseException as close_error:
            if error is None:raise
            error.add_note('Boundary close failed: '+str(close_error))
        return False

PLAN_PINS = ('wrapper','custody_owner','contract','parent_python','powershell','checked_guard',
    'metadata_receipt','image_packet','native_build_receipt','native_image','request',
    'native_input','profile','descriptor','graph_preserver','graph_provenance_owner',
    'graph_provenance_contract','graph_output_guard','output_guard','output_guard_lock')
BINDING_PINS = ('environment','source_roster','archive','producer','manifest',
    'runtime_producer','cargo_json','preserver_source')
PROOF_PINS = ('build_receipt','producer_source','parent_producer','parent_build_proof',
    'recipe','new_source','new_object','new_session','new_aggregate')


def derive_boundary(plan,binding,recipe,raw,b,known=None):
    """Derive current consumed objects, never copy the historical input-seal census."""
    result={}
    def add(row):
        file_pin(row);k=key(row['path'])
        require(Path(row['path']).is_absolute(),'absolute boundary path required')
        if k in result:
            require(result[k]==row,'conflicting graph boundary pin')
        else:result[k]=dict(row)
    def sealed(path,sha=None):
        path=str(Path(path));expected=sha if sha is not None else plan['input_seals'].get(path)
        require(isinstance(expected,str),'current boundary input lacks original seal: '+path)
        if known is not None:
            require(key(path) in known,'prepared boundary omitted derived input: '+path)
            row=known[key(path)]['pin']
        else:row=b.pin(path)
        require(row['path']==path and row['sha256']==expected,'current boundary seal differs')
        add(row)
    for field in PLAN_PINS:add(plan[field])
    for field in BINDING_PINS:add(binding[field])
    for row in binding['source_pins']:add(row)
    proof=binding['proof']
    for field in PROOF_PINS:add(proof[field])
    for row in proof['source_patch'].values():add(row)
    for row in recipe['helper_pins']:add(row)
    for row in raw['commands']:add(row['stdout'])
    for row in plan['runtime_files']:add(row)
    for relative,sha in plan['required_native_sources'].items():
        require(relative.startswith('crates/') and '\\' not in relative
                and not any(x in ('','.','..') for x in relative.split('/')),'invalid native source path')
        sealed(ROOT/relative,sha)
    for leaf,sha in b.HELPERS.items():
        require(Path(plan['ownership_helper_root'])==b.MAINTAINED_HELPER_ROOT,
                'graph execution requires maintained ownership helpers')
        sealed(b.MAINTAINED_HELPER_ROOT/leaf,sha)
    metadata=b.read(plan['metadata_receipt']['path'])
    add(metadata['plan'])
    old=b.read(metadata['plan']['path'])
    add(old['native_build_receipt'])
    for row in metadata['outputs']:add(row)
    native=b.read(plan['native_input']['path'])
    for path in native['files'].values():sealed(path)
    for field in ('source_manifest','numeric_fixture'):
        row=native[field];sealed(row['path'],row['sha256'])
    fixture=b.read(native['numeric_fixture']['path'])
    row=fixture['reference_producer'];sealed(row['path'],row['sha256'])
    # Runtime/source relationships above select the boundary. Extra historical
    # headers, old objects and unchanged parent archives stay derivation evidence.
    for row in result.values():
        declared=plan['input_seals'].get(row['path'])
        if declared is not None:require(declared==row['sha256'],'plan seal conflicts with derived boundary')
    require(0<len(result)<=MAX_FILES-2,'graph boundary cap includes receipt and plan')
    return [result[k] for k in sorted(result)]


def execution_base(lease):
    """Private maintained owner instance; all input reads/imports use held objects."""
    b=lease.load(ROOT/'scripts/cuda-native-execution.py','graph_held_native_owner')
    b.pin=lease.pin
    b.digest=lease.digest
    b.read=lambda path,limit=MAX_JSON:lease.json(path,limit)
    b.load=lease.load
    def pinned(row):
        require(lease.pin(row['path'])==row,'held input pin differs')
        return Path(row['path'])
    b.pinned=pinned
    return b


def held_preserver(lease,plan):
    owner=lease.load(plan['graph_preserver']['path'],'graph_held_preserver')
    owner.pin=lease.pin
    # Clone only the two bounded PE readers' globals. Their file interface reads
    # the existing retained HANDLE, never a newly opened path or copied PE image.
    class Reader:
        def __init__(self,path):
            self.path=Path(path);lease.pin(self.path);self.opened=lease.files[key(path)]
        def __enter__(self):self.opened.rewind();return self
        def __exit__(self,*args):return False
        def read(self,count):
            check(lease.deadline);require(type(count)is int and 0<=count<=1<<20,'PE read cap')
            data=self.opened.read(count);check(lease.deadline);return data
        def seek(self,offset):
            check(lease.deadline);require(type(offset)is int and 0<=offset<=lease.pin(self.path)['bytes'],'PE seek bound')
            self.opened.seek(offset)
    class HeldPath:
        def __init__(self,path):self.path=Path(path)
        def open(self,mode):
            require(mode=='rb','held PE read only')
            return Reader(self.path)
        def stat(self):return types.SimpleNamespace(st_size=lease.pin(self.path)['bytes'])
    scope=dict(owner.exported_collectors.__globals__,Path=HeldPath)
    pe=types.FunctionType(owner.pe.__code__,scope,'pe',owner.pe.__defaults__)
    scope['pe']=pe
    exports=types.FunctionType(owner.exported_collectors.__code__,scope,'exported_collectors',
                               owner.exported_collectors.__defaults__)
    exports.__kwdefaults__=owner.exported_collectors.__kwdefaults__
    owner.exported_collectors=exports
    return owner


def output_policy_owner(lease,plan):
    """Keep the original policy logic; pin its imports/tools and original cutoff."""
    owner=lease.load(plan['graph_output_guard']['path'],'graph_protected_output_guard')
    owner.digest=lease.digest
    def cpu_owner(lock):
        source=ROOT/'scripts/provision-static-onnx-runtime.py'
        lock_path=ROOT/'scripts/static-onnx-runtime-lock.json'
        require(lease.digest(source)==lock['cpu_owner_sha256']
                and lease.digest(lock_path)==lock['cpu_lock_sha256'],'canonical CPU policy owner drift')
        cpu=lease.load(source,'graph_cpu_policy_owner')
        current=lease.json(lock_path)
        require(cpu.LOCK==current,'CPU policy consumed lock differs')
        cpu.validate_lock(current)
        return cpu
    def run(argv,**kwargs):
        check(lease.deadline)
        argv=list(argv)
        if argv[0]=='pwsh':argv[0]=plan['powershell']['path']
        budget=min(kwargs.get('timeout',10),lease.deadline-time.monotonic())
        require(budget>0,'output policy original deadline expired')
        kwargs['timeout']=budget
        result=subprocess.run(argv,**kwargs)
        check(lease.deadline)
        return result
    owner.cpu_owner=cpu_owner
    owner.subprocess=types.SimpleNamespace(run=run)
    return owner


def replay(plan,binding,lease,preserver):
    """Replay the verified derivation data against currently held artifacts."""
    check(lease.deadline)
    require(binding.get('schema')=='airk.private.cuda-finalgraph-image-binding/1',
            'exact graph binding schema required')
    proof=binding['proof'];aggregate=binding['aggregate']
    for row in [binding[k] for k in BINDING_PINS]+binding['source_pins']:
        require(lease.pin(row['path'])==row,'current graph binding pin differs')
    require(binding['preserver_source']==plan['graph_preserver']
            and binding['producer']==proof['build_receipt']
            and binding['archive']==proof['new_aggregate'],'current producer/archive/source binding differs')
    env=lease.json(binding['environment']['path'])
    raw=lease.json(proof['build_receipt']['path']);recipe=lease.json(proof['recipe']['path'])
    require(env['schema']=='airk.cuda-static-environment/5' and env['build']==proof
        and env['aggregate']==aggregate and env['activation_pins_available'] is False,'prepared environment relationship differs')
    require(proof['schema']=='airk.cuda-static-build-proof/3' and proof['qualification'] is False
        and proof['activation_pins_available'] is False and proof['cold_build_qualified'] is False,
        'prepared build scope differs')
    owner=lease.load(ROOT/'scripts/cuda-static-incremental.py','graph_prepared_incremental')
    require(raw['schema']==owner.RAW and raw['status']==owner.PASS and raw['failure'] is None
        and raw['input_recheck_errors']==[] and raw['limits']==owner.LIMITS
        and raw['qualification'] is False and raw['cold_build_qualified'] is False
        and raw['activation_pins_available'] is False,'actual prepared producer result differs')
    require(raw['mode']=='incremental-finalgraph' and raw['producer_source']==recipe['producer_source']
            and raw['producer_source']==lease.pin(ROOT/'scripts/cuda-static-incremental.py')
            and proof['recipe']==raw['recipe'],'current incremental producer source/recipe differs')
    require(owner.validate_recipe(recipe)==Path(recipe['fresh_output'])
            and Path(recipe['repo_root'])==ROOT,'current recipe root differs')
    owner.validate_clocks(raw,recipe)
    require(raw['execution_user_sid']==raw['default_owner_sid']==SID
            and raw['identity']['UserSid']==raw['identity']['OwnerSid']==SID
            and SID in raw['identity']['InteractiveOwnerSids']
            and type(raw['identity']['SessionId'])is int and raw['identity']['SessionId']>0
            and raw['timing_scope']==owner.TIMING,'prepared producer identity/timing differs')
    require(proof['build_receipt']['sha256']!=owner.PARENT
            and proof['new_aggregate']['sha256']!=owner.AGGREGATE
            and proof['new_source']['sha256']==recipe['source_patch']['after']['sha256'],
            'fresh producer/source/aggregate identity absent')
    for field,recipe_field in [('parent_producer','old_producer'),('parent_build_proof','parent_build_proof'),
                                ('parent_aggregate','old_aggregate'),('source_patch','source_patch')]:
        require(raw[field]==recipe[recipe_field],'producer recipe relationship differs: '+field)

    for field in ('producer_source','parent_producer','parent_build_proof','parent_aggregate','source_patch',
                  'new_source','new_object','new_session','new_aggregate','session_input_pins','aggregate_input_pins'):
        require(raw[field]==proof[field],'prepared producer projection differs: '+field)
    require(proof['recipe']==raw['recipe'] and proof['compile_dependencies']==recipe['compiler_dependencies'],
            'prepared recipe/dependency relationship differs')
    for command in raw['commands']:
        tool=next(r for r in recipe['compiler_tool_files'] if r['path']==command['argv'][0])
        owner.settled(command['owned'],tool)
    parent=lease.json(proof['parent_build_proof']['path'])
    require(parent['schema']=='airk.cuda-static-build-proof/2'
        and parent['build_receipt']['sha256']==proof['parent_producer']['sha256']==owner.PARENT
        and proof['parent_aggregate']['sha256']==owner.AGGREGATE,'historical ancestry labels differ')
    owner.validate_parent_toolchain(recipe,parent)
    for field in owner.ANCESTRY:
        require(proof[field]==parent[field],'inherited ancestry metadata differs')
    require(aggregate['producer_proof_sha256']==owner.canonical(proof)
        and aggregate['receipt_sha256']==proof['build_receipt']['sha256']
        and aggregate['plan_sha256']==owner.canonical(recipe)
        and aggregate['coff']==raw['archive_proof']['aggregate']
        and aggregate['output']==dict(path='aggregate/onnxruntime.lib',bytes=proof['new_aggregate']['bytes'],
                                     sha256=proof['new_aggregate']['sha256'])
        and aggregate['qualification'] is False,'actual aggregate relationship differs')
    require(lease.bytes(plan['graph_output_guard']['path'])==
            b'# SPDX-License-Identifier: Apache-2.0\n'+lease.bytes(plan['output_guard']['path']),
            'maintained protected-output guard body differs from historical evidence')
    manifest=lease.json(plan['descriptor']['path'])
    actual=preserver.validate_graph_manifest(manifest,plan['descriptor'],Path(plan['descriptor']['path']).parent,
        binding,deadline=lease.deadline)
    require(actual==binding['runtime_producer'],'prepared native runtime producer differs')
    require(binding['build_receipt']==plan['native_build_receipt'] and binding['manifest']==plan['descriptor'],
            'prepared native build/manifest differs')
    ort,_=preserver.cargo_rows(lease.bytes(binding['cargo_json']['path'],64<<20).decode('utf-8'),
                              Path(binding['archive']['path']))
    require(ort==binding['actual_static_ORT_rows'],'prepared actual Cargo link differs')
    require(preserver.exported_collectors(plan['native_image']['path'],deadline=lease.deadline)
        ==binding['image_exports'].get(plan['native_image']['sha256']),'actual prepared image collectors differ')
    source_roster=lease.json(binding['source_roster']['path'])
    require(set(source_roster)==set(preserver.PROVENANCE_OWNERS)
        and {str(ROOT/k):v for k,v in source_roster.items()}
        =={r['path']:r['sha256'] for r in binding['source_pins']},'prepared verifier source roster differs')
    environment=lease.load(ROOT/'scripts/cuda-runtime-environment.py','graph_environment_relationships')
    require(env['environment']==environment.environment(recipe['fresh_output'],proof,aggregate),
            'current static environment values differ')
    require(set(env['request'])=={'destination','incremental_root','incremental_proposal','receipt_sha256'}
            and env['request']['receipt_sha256']==proof['build_receipt']['sha256']
            and Path(env['request']['destination'])==Path(recipe['fresh_output'])
            and Path(env['request']['incremental_root'])==Path(recipe['fresh_output'])
            and Path(env['request']['incremental_proposal'])==Path(proof['recipe']['path']).parent,
            'current environment destination/producer differs')
    check(lease.deadline)
    return recipe,raw

def read_pinned(pin,b):
    file_pin(pin)
    require(pin['bytes']<=MAX_JSON,'preparation bootstrap JSON cap')
    require(b.pin(pin['path'])==pin,'preparation bootstrap pin drift')
    raw=Path(pin['path']).read_bytes()
    require(len(raw)==pin['bytes'] and hashlib.sha256(raw).hexdigest()==pin['sha256'],
            'preparation bootstrap consumed bytes differ')
    return strict_json(raw)


def execution_plan(prepared,preparation_pin):
    require(prepared['schema']==SCHEMA and prepared['status']=='PREPARED_UNQUALIFIED'
            and prepared['qualification'] is False and prepared['product_admission'] is False
            and prepared['runtime_admitted'] is False and prepared['whole_build_reverified'] is True
            and prepared['handle_close_errors']==[],'reviewed preparation evidence absent')
    a,z,d=(prepared[k] for k in ('started_monotonic','finished_monotonic','preparation_deadline_monotonic'))
    require(all(type(x)in(int,float) and math.isfinite(x) for x in (a,z,d))
            and a<=z<d<=a+600 and prepared['elapsed_seconds']==z-a,'preparation clock differs')
    observed=prepared['identity']
    require(observed['UserSid']==observed['OwnerSid']==SID and SID in observed['InteractiveOwnerSids']
            and type(observed['SessionId'])is int and observed['SessionId']>0,'preparation human identity differs')
    timestamps=[datetime.datetime.fromisoformat(prepared[k]) for k in
                ('started_utc','finished_utc','original_deadline')]
    require(all(t.utcoffset()==datetime.timedelta(0) for t in timestamps)
            and timestamps[0]<=timestamps[1]<timestamps[2]
            and abs((timestamps[2]-timestamps[0]).total_seconds()-(d-a))<0.000002,
            'preparation UTC chronology differs')
    require(prepared['opened_handles']==prepared['closed_handles']==len(prepared['boundary']),
            'preparation handle settlement incomplete')
    plan=copy.deepcopy(prepared['plan'])
    require(prepared['preparation_source']==plan['graph_provenance_owner']
            and prepared['parent_source']==plan['wrapper']
            and prepared['preserver_source']==plan['graph_preserver']
            and prepared['metadata_receipt']==plan['metadata_receipt']
            and prepared['image_packet']==plan['image_packet'],'preparation plan bindings differ')
    seals={}
    for row in prepared['boundary']:
        require(set(row)=={'pin','identity','policy'},'prepared boundary row differs')
        pin=file_pin(row['pin'])
        require(pin['path'] not in seals,'duplicate prepared boundary')
        seals[pin['path']]=pin['sha256']
    file_pin(preparation_pin);require(preparation_pin['path'] not in seals,'preparation aliases an input')
    seals[preparation_pin['path']]=preparation_pin['sha256']
    plan['input_seals']=seals
    plan['graph_preparation']=preparation_pin
    return plan


def prepare(plan,b,receipt_path,c,started,started_utc,deadline,*,opener=WindowsFile,observe=policies):
    check(deadline)
    require(not Path(receipt_path).exists(),'existing preparation receipt preserved')
    c.guard(plan,receipt_path,deadline=deadline)
    packet=read_pinned(plan['image_packet'],b);binding=packet['static_graph_binding']
    proof=binding['proof'];recipe=read_pinned(proof['recipe'],b);raw=read_pinned(proof['build_receipt'],b)
    pins=derive_boundary(plan,binding,recipe,raw,b)
    with Lease(plan,pins,deadline,opener=opener,observe=observe) as lease:
        # Full maintained ancestry verification is deliberately preparation-only.
        # Current verifier files are held immutable while their normal loaders run.
        c.preflight(plan,b,before=True,deadline=deadline)
        replay(plan,binding,lease,held_preserver(lease,plan))
        lease.refresh()
        records=lease.records()
    finished_utc = datetime.datetime.now(datetime.timezone.utc)
    finished = time.monotonic()
    require(finished < deadline, 'original graph provenance deadline expired')
    value=dict(schema=SCHEMA,status='PREPARED_UNQUALIFIED',plan=plan,
        preparation_source=plan['graph_provenance_owner'],parent_source=plan['wrapper'],
        preserver_source=plan['graph_preserver'],metadata_receipt=plan['metadata_receipt'],
        image_packet=plan['image_packet'],binding=binding,boundary=records,
        loaded_sources=list(lease.loaded.values()),identity=lease.identity,
        started_monotonic=started,finished_monotonic=finished,elapsed_seconds=finished-started,
        started_utc=started_utc.isoformat(),finished_utc=finished_utc.isoformat(),
        original_deadline=(started_utc+datetime.timedelta(seconds=deadline-started)).isoformat(),
        preparation_deadline_monotonic=deadline,qualification=False,product_admission=False,
        whole_build_reverified=True,runtime_admitted=False,opened_handles=len(lease.files),
        closed_handles=lease.closed_count,handle_close_errors=lease.close_errors,
        receipt_retention_excluded_from_elapsed=True)
    c.write_new(plan,Path(receipt_path),value)
    # Receipt retention has the existing separately bounded checked-guard policy.
    # A failed write/audit never returns a reviewable preparation pin.
    return b.pin(receipt_path)


def open_prepared(plan,plan_pin,b,deadline,*,opener=WindowsFile,observe=policies):
    check(deadline)
    prepared=read_pinned(plan['graph_preparation'],b)
    require(plan==execution_plan(prepared,plan['graph_preparation']),'graph plan differs from exact prepared plan')
    expected={}
    for row in prepared['boundary']:
        k=key(row['pin']['path']);require(k not in expected,'duplicate prepared object path')
        expected[k]=row
    seed=prepared['plan'];binding=prepared['binding']
    recipe=read_pinned(binding['proof']['recipe'],b);raw=read_pinned(binding['proof']['build_receipt'],b)
    pins=derive_boundary(seed,binding,recipe,raw,b,expected)
    require({key(r['path']) for r in pins}==set(expected),'prepared boundary has omissions or extras')
    pins += [plan['graph_preparation']]
    if plan_pin is not None:pins.append(plan_pin)
    lease=Lease(plan,pins,deadline,expected=expected,opener=opener,observe=observe)
    try:
        require(lease.json(plan['graph_preparation']['path'])==prepared,'held preparation differs from bootstrap')
        current=lease.json(plan['image_packet']['path'])
        require(current.get('static_graph_binding')==binding,'actual image packet binding differs')
        held=execution_base(lease)
        require(derive_boundary(seed,binding,lease.json(binding['proof']['recipe']['path']),
                lease.json(binding['proof']['build_receipt']['path']),held,expected)
                ==pins[:len(expected)],'held boundary data differs from bootstrap')
        replay(plan,binding,lease,held_preserver(lease,plan))
        lease.plan_pin = plan_pin
        lease.binding = binding
        lease.prepared = prepared
        lease.base = held
        return lease
    except BaseException as error:
        try:lease.close()
        except BaseException as close_error:error.add_note('Boundary close failed: '+str(close_error))
        raise


def finish_lease(lease,receipt):
    """Run after output proofs, before the final original deadline observation."""
    try:
        lease.refresh()
    finally:
        try:lease.close()
        finally:
            receipt['graph_custody']=dict(opened_handles=len(lease.files),closed_handles=lease.closed_count,
                close_errors=list(lease.close_errors),boundary=lease.records(),
                loaded_sources=list(lease.loaded.values()),preparation=lease.plan['graph_preparation'])

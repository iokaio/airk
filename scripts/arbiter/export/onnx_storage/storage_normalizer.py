# SPDX-License-Identifier: Apache-2.0
"""Artifact storage only: native external data belongs to main direct initializers.

Call only for a NEW unqualified output, never a previously frozen input.
Original graph bytes and a complete storage/value proof are retained separately.
No model, Torch or inference runtime is imported or executed here.
"""
import copy
import hashlib
import math
from pathlib import Path, PureWindowsPath

MAX_GRAPH=64<<20

def sha(path):
    with Path(path).open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def tensors(message,path=()):
    if message.DESCRIPTOR.full_name=='onnx.TensorProto':yield path,message
    for field,value in message.ListFields():
        if field.message_type is not None:
            if field.is_repeated:
                for index,child in enumerate(value):yield from tensors(child,path+(field.name,index))
            else:yield from tensors(value,path+(field.name,))

def main_initializer(path):return len(path)==3 and path[:2]==('graph','initializer')

def location(root,name):
    if not name or name in ['.','..']or len(name)>128 or '/'in name or '\\'in name or PureWindowsPath(name).drive or ':'in name:
        raise ValueError('External data must be one portable local filename')
    if any(not(c.isascii()and(c.isalnum()or c in '-_.'))for c in name):raise ValueError('External data filename differs from native asset contract')
    path=root/name;metadata=path.stat(follow_symlinks=False)
    if path.is_symlink()or not path.is_file()or getattr(metadata,'st_file_attributes',0)&0x400:raise ValueError('External data aliases another path')
    return path

def external_range(root,tensor):
    fields={}
    for entry in tensor.external_data:
        if entry.key in fields or entry.key not in ['location','offset','length','checksum']:raise ValueError('Ambiguous external tensor fields')
        fields[entry.key]=entry.value
    if tensor.HasField('raw_data')or any(len(getattr(tensor,name))for name in ['float_data','int32_data','string_data','int64_data','double_data','uint64_data'])or 'location'not in fields:
        raise ValueError('External tensor also has inline data or lacks location')
    path=location(root,fields['location']);size=path.stat().st_size
    def number(key,default):
        value=fields.get(key,str(default))
        if not value.isascii()or not value.isdecimal():raise ValueError('External range must be unsigned decimal')
        return int(value)
    offset=number('offset',0);length=number('length',size-offset)
    if length<=0 or offset>size or length>size-offset:raise ValueError('External tensor exceeds exact source file')
    return path,offset,length

def expected_bytes(tensor,onnx):
    T=onnx.TensorProto
    widths={T.FLOAT:32,T.UINT8:8,T.INT8:8,T.UINT16:16,T.INT16:16,T.INT32:32,T.INT64:64,T.BOOL:8,
            T.FLOAT16:16,T.DOUBLE:64,T.UINT32:32,T.UINT64:64,T.COMPLEX64:64,T.COMPLEX128:128,T.BFLOAT16:16,
            T.FLOAT8E4M3FN:8,T.FLOAT8E4M3FNUZ:8,T.FLOAT8E5M2:8,T.FLOAT8E5M2FNUZ:8,T.UINT4:4,T.INT4:4,T.FLOAT4E2M1:4,T.FLOAT8E8M0:8}
    if tensor.data_type not in widths or any(d<0 for d in tensor.dims):raise ValueError('Unsupported inline tensor numeric type or shape')
    return (math.prod(tensor.dims)*widths[tensor.data_type]+7)//8

def normalize_external_storage(graph_path,*,original_graph_path,proof_path):
    """Normalize a fresh candidate graph in place; preserve backup and proof outside its closure."""
    import json
    import onnx
    graph_path=Path(graph_path).absolute();metadata=graph_path.stat(follow_symlinks=False)
    if graph_path.is_symlink()or getattr(metadata,'st_file_attributes',0)&0x400:raise ValueError('Graph alias refused')
    graph_path=graph_path.resolve();backup=Path(original_graph_path).resolve();proof_path=Path(proof_path).resolve()
    if backup.exists()or proof_path.exists()or graph_path in [backup,proof_path]or backup==proof_path:raise ValueError('Fresh distinct original/proof paths required')
    if graph_path.is_symlink()or not graph_path.is_file()or graph_path.stat().st_size>MAX_GRAPH:raise ValueError('Original graph type/64MiB bound differs')
    original=graph_path.read_bytes();model=onnx.load_model_from_string(original)
    if model.functions or model.training_info:raise ValueError('Native product graphs cannot carry local functions/training graphs')
    canonical=model.SerializeToString(deterministic=True)
    before_main=[tensor.SerializeToString(deterministic=True)for tensor in model.graph.initializer]
    files={};main_files=set();changes=[];original_tensors={}
    for path,tensor in list(tensors(model)):
        if tensor.external_data and tensor.data_location!=onnx.TensorProto.EXTERNAL:raise ValueError('External metadata lacks external storage kind')
        if tensor.data_location!=onnx.TensorProto.EXTERNAL:continue
        file,offset,length=external_range(graph_path.parent,tensor)
        if str(file)not in files:files[str(file)]=sha(file)
        if main_initializer(path):main_files.add(str(file));continue
        if length>MAX_GRAPH or expected_bytes(tensor,onnx)!=length:raise ValueError('Inline tensor size/type/shape disagrees with exact external range')
        with file.open('rb')as stream:stream.seek(offset);payload=stream.read(length)
        if len(payload)!=length:raise ValueError('External tensor changed during bounded read')
        original_tensors[path]=copy.deepcopy(tensor)
        changes.append({'path':list(path),'name':tensor.name,'element':tensor.data_type,'shape':list(tensor.dims),
            'source_file':str(file),'source_file_sha256':files[str(file)],'offset':offset,'length':length,'value_sha256':hashlib.sha256(payload).hexdigest()})
        tensor.raw_data=payload;tensor.ClearField('external_data');tensor.ClearField('data_location')
    if not main_files or set(files)!=main_files:raise ValueError('Every retained external file must still belong to a main initializer; no orphan file migration')
    if [tensor.SerializeToString(deterministic=True)for tensor in model.graph.initializer]!=before_main:raise ValueError('Main initializer declarations changed')
    normalized=model.SerializeToString(deterministic=True)
    if len(normalized)>MAX_GRAPH:raise ValueError('Normalized graph exceeds native64MiB cap')
    restored=copy.deepcopy(model)
    actual_by_path=dict(tensors(restored))
    for path,original_tensor in original_tensors.items():actual_by_path[path].CopyFrom(original_tensor)
    if restored.SerializeToString(deterministic=True)!=canonical:raise ValueError('Model topology or other tensor fields changed')
    if any(sha(file)!=expected for file,expected in files.items()):raise ValueError('External data source drift')
    backup.parent.mkdir(parents=True,exist_ok=True);proof_path.parent.mkdir(parents=True,exist_ok=True)
    backup.write_bytes(original)
    temporary=graph_path.with_name(graph_path.name+'.native-storage.tmp')
    if temporary.exists():raise ValueError('Fresh temporary graph required')
    temporary.write_bytes(normalized);temporary.replace(graph_path)
    observed=onnx.load_model(str(graph_path),load_external_data=False)
    if observed.SerializeToString(deterministic=True)!=normalized:raise ValueError('Written graph differs from proved metadata')
    after_tensors=dict(tensors(observed))
    for change in changes:
        tensor=after_tensors[tuple(change['path'])]
        if tensor.data_location==onnx.TensorProto.EXTERNAL or tensor.external_data or hashlib.sha256(tensor.raw_data).hexdigest()!=change['value_sha256']:
            raise ValueError('Written inline value differs from original external payload')
    if any(not main_initializer(path)for path,tensor in tensors(observed)if tensor.data_location==onnx.TensorProto.EXTERNAL):raise ValueError('Unsupported external tensor remains')
    if any(sha(file)!=expected for file,expected in files.items()):raise ValueError('Main weight files changed')
    onnx.checker.check_model(str(graph_path),full_check=False)
    proof={'schema':'airk.private.native-onnx-storage-proof/1','complete':True,'math_or_precision_changed':False,'model_executed':False,
        'original_graph':str(backup),'original_graph_sha256':hashlib.sha256(original).hexdigest(),
        'normalized_graph':str(graph_path),'normalized_graph_sha256':sha(graph_path),'normalized_graph_bytes':len(normalized),
        'main_initializer_count':len(before_main),'main_initializer_declarations_identical':True,'topology_restored_identity':True,
        'external_files_before_after_sha256':files,'inlined':changes,'remaining_external_main_tensors':sum(t.data_location==onnx.TensorProto.EXTERNAL for t in observed.graph.initializer)}
    proof_path.write_text(json.dumps(proof,indent=2)+'\n',encoding='utf8')
    return proof

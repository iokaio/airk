# SPDX-License-Identifier: Apache-2.0
"""Exact external-data closure, without loading the trained weights into Python."""
import hashlib
import os
from pathlib import Path, PureWindowsPath

def digest(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def local_file(root,name):
    if not isinstance(name,str) or not name or name in ['.','..'] or '/' in name or '\\' in name or PureWindowsPath(name).drive or ':' in name:
        raise ValueError('External weight must be one local portable filename')
    path=root/name
    if path.is_symlink() or not path.is_file() or getattr(path.stat(follow_symlinks=False),'st_file_attributes',0)&0x400:
        raise ValueError('External weight missing or aliases another path')
    return path

def bind_references(root,references,expected_extra=()):
    root=Path(root).resolve();files={'model.onnx'};ranges=[]
    local_file(root,'model.onnx')
    for tensor,entries in references:
        fields={}
        for key,value in entries:
            if key in fields or key not in ['location','offset','length','checksum']:
                raise ValueError('Ambiguous or unsupported external data metadata')
            fields[key]=value
        if 'location' not in fields:raise ValueError('Missing external data location')
        path=local_file(root,fields['location']);size=path.stat().st_size
        def number(key,default):
            value=fields.get(key,str(default))
            if not value.isascii() or not value.isdecimal():raise ValueError('External range is not an unsigned decimal')
            return int(value)
        offset=number('offset',0);length=number('length',size-offset)
        if length<=0 or offset>size or length>size-offset:raise ValueError('External data range exceeds exact sidecar')
        files.add(path.name);ranges.append(dict(tensor=tensor,location=path.name,offset=offset,length=length))
    actual={p.name for p in root.iterdir()}
    if actual!=files|set(expected_extra):raise ValueError('Unreferenced/missing export file in graph closure')
    return {'files':[dict(name=name,bytes=(root/name).stat().st_size,sha256=digest(root/name))for name in sorted(files)],'external_ranges':ranges}

def graph_closure(root,expected_extra=()):
    import onnx
    root=Path(root).resolve()
    graph=onnx.load_model(str(local_file(root,'model.onnx')),load_external_data=False)
    references=[]
    def visit(message):
        if message.DESCRIPTOR.full_name=='onnx.TensorProto' and message.data_location==onnx.TensorProto.EXTERNAL:
            references.append((message.name,[(entry.key,entry.value)for entry in message.external_data]))
        for field,value in message.ListFields():
            if field.message_type is not None:
                if field.is_repeated:
                    for child in value:visit(child)
                else:visit(value)
    visit(graph)
    if not references:raise ValueError('Full trained FP32 graph must use explicit external weights')
    result=bind_references(root,references,expected_extra)
    expected=[('input_ids',onnx.TensorProto.INT64,[1,128]),('attention_mask',onnx.TensorProto.INT64,[1,128]),('position_ids',onnx.TensorProto.INT64,[1,128])]
    def abi(value):return value.name,value.type.tensor_type.elem_type,[d.dim_value if d.HasField('dim_value') else None for d in value.type.tensor_type.shape.dim]
    if [abi(value)for value in graph.graph.input]!=expected or [abi(value)for value in graph.graph.output]!=[('logits',onnx.TensorProto.FLOAT,[1,2])]:
        raise ValueError('Exported graph has different static tensor ABI')
    onnx.checker.check_model(str(root/'model.onnx'),full_check=False)
    return result

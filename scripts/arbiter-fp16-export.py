# SPDX-License-Identifier: Apache-2.0
"""Private G5 proposal. No CLI, Torch, ORT, GPU or implicit graph execution.
Caller must own the reviewed FP32 release, native G5 policy, deadline and output.
ONNX/numpy are injected so import of this module performs no third-party work.
"""
import copy, hashlib, heapq, math, re, struct, time
from pathlib import Path

ORIGINAL = {
    'focus': '19b20ad4e39e173f6da602094b9c1583726a65fa76742553f97ec6f0f58436a1',
    'compact': 'c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4',
    'large': 'f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94',
}
GEOMETRY = {'focus': (28, 16, 8, 1024), 'compact': (28, 16, 8, 2048), 'large': (36, 32, 8, 2560)}
NS = '/airk_g5/'
PROJECTIONS = {'q_proj', 'k_proj', 'v_proj', 'o_proj', 'gate_proj', 'up_proj', 'down_proj'}
RECIPES = ('g5', 'portable')

def require(ok, message):
    if not ok: raise ValueError(message)

def sha(raw): return hashlib.sha256(raw).hexdigest()

def half_literal(raw):
    """Independent small-reference encoder; production uses bounded numpy chunks."""
    require(len(raw) % 4 == 0, 'unaligned FP32')
    out = bytearray()
    for (value,) in struct.iter_unpack('<f', raw):
        require(math.isfinite(value), 'nonfinite FP32')
        try: out.extend(struct.pack('<e', value))
        except OverflowError as exc: raise ValueError('binary16 overflow') from exc
    return bytes(out)

def finite_f32_chunk(raw, np):
    require(len(raw) % 4 == 0, 'unaligned FP32')
    values = np.frombuffer(raw, dtype='<f4')
    require(bool(np.isfinite(values).all()), 'nonfinite FP32 learned weight')
    return values

def half_chunk(raw, np):
    values = finite_f32_chunk(raw, np)
    with np.errstate(over='ignore', invalid='ignore'):
        converted = values.astype('<f2')
    require(bool(np.isfinite(converted).all()), 'binary16 overflow')
    return converted.tobytes()

def validate_inputs(model, mask, positions):
    require(model in GEOMETRY, 'unknown model')
    require(all(type(x) is int and x in (0, 1) for x in mask), 'binary mask required')
    require(positions == list(range(len(mask))) and all(type(x) is int for x in positions), 'position rule differs')
    if model == 'focus':
        require(len(mask) == 512 and 1 in mask, 'Focus static nonempty512 required')
        pad = mask.index(1); live = 512 - pad
        require(mask == [0] * pad + [1] * live, 'holes/right padding forbidden')
        return [live, pad, 512, pad, 512]
    require(0 < len(mask) <= 8192 and all(mask), 'Context all-live1..8192 required')
    return [len(mask)-1], [len(mask)]

def attributes(node, onnx):
    return {a.name: onnx.helper.get_attribute_value(a) for a in node.attribute}

def classify(model, key):
    """Complete role census from exact original graph; unknown weights refuse."""
    layers, heads, kv, hidden = GEOMETRY[key]
    uses = {}
    for n in model.graph.node:
        for i, v in enumerate(n.input): uses.setdefault(v, []).append((n, i))
    result = {}; projections = set(); norms = 0; embeddings = 0; readouts = 0
    for t in model.graph.initializer:
        require(t.name not in result and t.data_type == 1, 'duplicate/non-FP32 initializer')
        consumers = uses.get(t.name, [])
        require(consumers, 'unused source initializer')
        if t.name.endswith('embed_tokens.weight'):
            require(len(t.dims) == 2 and t.dims[1] == hidden and all(n.op_type == 'Gather' and i == 0 for n,i in consumers), 'embedding role differs')
            role = 'embedding_half'; embeddings += 1
        elif t.name.endswith(('norm.weight', 'layernorm.weight')):
            require(len(t.dims) == 1 and all(n.op_type == 'Mul' for n,i in consumers), 'norm role differs')
            role = 'norm_fp32'; norms += 1
        elif key == 'focus' and list(t.dims) == [hidden, 2]:
            require(len(consumers) == 1 and consumers[0][0].name == '/MatMul' and consumers[0][1] == 1, 'Focus head differs')
            role = 'readout_fp32'; readouts += 1
        else:
            require(len(consumers) == 1, 'aliased learned projection')
            n, i = consumers[0]
            match = re.fullmatch(r'/(?:base|scorer)/layers\.(\d+)/(?:self_attn|mlp)/(\w+)/MatMul', n.name)
            require(match is not None and n.op_type == 'MatMul' and i == 1 and len(t.dims) == 2, 'unknown learned parameter/bias')
            layer, projection = int(match[1]), match[2]
            require(layer < layers and projection in PROJECTIONS and (layer,projection) not in projections, 'projection census differs')
            projections.add((layer,projection)); role = 'projection_half'
        result[t.name] = role
    require(len(projections) == layers*7 and norms == layers*4+1 and embeddings == 1 and readouts == (key=='focus'), 'incomplete parameter census')
    require(len(result) == layers*11+2+(key=='focus'), 'initializer count differs')
    return result

def reachable(nodes, outputs, known):
    names = {}; produced = {}
    for n in nodes:
        require(n.name and n.name not in names, 'duplicate node name')
        names[n.name] = n
        for v in n.output:
            if v:
                require(v not in produced and v not in known, 'duplicate producer')
                produced[v] = n.name
    wanted = set(); todo = list(outputs)
    while todo:
        v = todo.pop()
        if not v or v in known: continue
        require(v in produced, 'unbound tensor '+v)
        name = produced[v]
        if name not in wanted: wanted.add(name); todo.extend(names[name].input)
    deps = {n:{produced[v] for v in names[n].input if v in produced} for n in wanted}
    children = {n:[] for n in wanted}
    for n, parents in deps.items():
        for parent in parents: children[parent].append(n)
    queue = [n for n in wanted if not deps[n]]; heapq.heapify(queue); ordered=[]
    while queue:
        name=heapq.heappop(queue); ordered.append(names[name])
        for child in children[name]:
            deps[child].remove(name)
            if not deps[child]: heapq.heappush(queue,child)
    require(len(ordered)==len(wanted), 'cycle')
    return ordered

def rewrite(model, key, onnx, recipe='g5'):
    """Metadata-only transformation; external arrays are converted separately.
    Recipe 'g5' is the CUDA proposal with half products and half attention.
    Recipe 'portable' keeps the same half storage and computes every product,
    activation and attention in FP32, for the CPU and portable GPU routes.
    """
    require(recipe in RECIPES, 'unknown recipe'); portable=recipe=='portable'
    require(key in GEOMETRY and not model.functions and not model.graph.sparse_initializer, 'unsupported graph')
    original=copy.deepcopy(model); g=model.graph; h=onnx.helper; T=onnx.TensorProto
    expected=['input_ids','attention_mask','position_ids']
    if key!='focus': expected+=['decision_positions','candidate_positions','option_mask']
    require([x.name for x in g.input]==expected and [x.name for x in g.output]==(['logits'] if key=='focus' else ['scores']), 'ABI differs')
    require(all(x.type.tensor_type.elem_type==T.INT64 for x in g.input[:3]) and all(x.type.tensor_type.elem_type==T.FLOAT for x in g.output), 'ABI types differ')
    require(not any(n.name.startswith(NS) or n.domain or any(a.type in (5,10) for a in n.attribute) for n in g.node), 'foreign namespace/custom source operator/subgraph')
    roles=classify(model,key); layers,heads,kv,hidden=GEOMETRY[key]
    nodes={n.name:n for n in g.node}; producers={v:n for n in g.node for v in n.output}
    require(len(nodes)==len(g.node) and len(producers)==sum(len(n.output) for n in g.node), 'duplicate source nodes/outputs')
    added=[]; constants=[]; modified=set(); removed=set(); scales=[]
    def const(name, dtype, dims, values):
        name=NS+name; constants.append(h.make_tensor(name,dtype,dims,values)); return name
    def node(op,inputs,name,**attrs):
        name=NS+name; added.append(h.make_node(op,inputs,[name],name=name,**attrs)); return name
    zero=const('float_zero',T.FLOAT,[],[0.0]) if portable else const('half_zero',T.FLOAT16,[],[0.0])
    axis1=const('axis1',T.INT64,[1],[1]); one=const('one',T.INT64,[1],[1]); axis2=const('axis2',T.INT64,[1],[2])
    shape=node('Shape',['attention_mask'],'input_shape'); total=node('Gather',[shape,axis1],'T',axis=0)
    total32=node('Cast',[total],'T32',to=T.INT32)
    if key=='focus':
        mask32=node('Cast',['attention_mask'],'mask32',to=T.INT32)
        live=node('ReduceSum',[mask32,axis1],'live',keepdims=0)
        pad=node('Sub',[total32,live],'pad')
        packed=node('Concat',[live,pad,total32,pad,total32],'packed_mask',axis=0)
        maskbool=node('Cast',['attention_mask'],'maskbool',to=T.BOOL)
        livequeries=node('Unsqueeze',[maskbool,axis2],'live_queries')
    else:
        before=node('Sub',[total,one],'Tminus1')
        seqlens=node('Cast',[before],'seqlens32',to=T.INT32)
    # Keep every original FP32 math node; only learned consumers gain half boundaries.
    for t in g.initializer:
        if not roles[t.name].endswith('_half'): continue
        t.data_type=T.FLOAT16
        consumers=[n for n in g.node if t.name in n.input]
        for n in consumers:
            original_output=n.output[0]; require(len(n.output)==1,'multi-output weight consumer')
            if n.op_type=='MatMul':
                if portable:
                    # Weight-only boundary: upcast the stored half weight. The activation and the product stay FP32.
                    require(n.input[1]==t.name,'unreviewed weight operand')
                    n.input[1]=node('Cast',[t.name],'weight/'+n.name+'/weight32',to=T.FLOAT)
                    modified.add(n.name)
                    continue
                value=node('Cast',[n.input[0]],'weight/'+n.name+'/input16',to=T.FLOAT16)
                n.input[0]=value
            else: require(n.op_type=='Gather' and n.input[0]==t.name, 'unreviewed half consumer')
            n.output[0]=NS+'weight/'+n.name+'/output16'
            added.append(h.make_node('Cast',[n.output[0]],[original_output],name=NS+'weight/'+n.name+'/output32',to=T.FLOAT))
            modified.add(n.name)
    for i in range(layers):
        prefix=('/base/' if key=='focus' else '/scorer/')+f'layers.{i}/self_attn/'
        qk=nodes[prefix+'MatMul']; soft=nodes[prefix+'Softmax']; masked=nodes[prefix+'Add_2']; av=nodes[prefix+'MatMul_1']
        mul=nodes[prefix+('Mul_6' if key=='focus' else 'Mul_8')]
        sink=nodes[prefix+('Reshape_5' if key=='focus' else 'Reshape_7')]
        require(qk.op_type=='MatMul' and qk.input[0]==prefix+'Add_output_0' and mul.op_type=='Mul' and mul.input[0]==qk.output[0], 'QK/scale topology differs')
        c=producers[mul.input[1]]; require(c.op_type=='Constant','nonliteral scale')
        tensor=next(a.t for a in c.attribute if a.name=='value')
        scale=onnx.numpy_helper.to_array(tensor)
        require(tensor.data_type==T.FLOAT and scale.size==1, 'scale dtype/shape differs')
        scale=float(scale.reshape(-1)[0]); require(math.isfinite(scale) and scale>0,'invalid scale')
        scales.append(struct.pack('<f',scale).hex())
        require(masked.op_type=='Add' and list(masked.input)==[mul.output[0],('/Where_output_0' if key=='focus' else '/scorer/Where_output_0')], 'mask differs')
        require(soft.op_type=='Softmax' and list(soft.input)==list(masked.output) and attributes(soft,onnx)=={'axis':-1}, 'softmax differs')
        require(av.op_type=='MatMul' and sink.op_type=='Reshape' and nodes[prefix+'Add_1'].op_type=='Add', 'attention output boundary differs')
        tag=f'attention/{i}/'
        q=node('Transpose',[prefix+'Add_output_0'],tag+'q_btnh',perm=[0,2,1,3])
        k=node('Transpose',[prefix+'Add_1_output_0'],tag+'k_btkh',perm=[0,2,1,3])
        v=prefix+'v_proj/MatMul_output_0'
        if key=='focus':
            # Gather the KV-head axis with [0,0,1,1,...], never tile [0,1,...].
            kvshape=const(tag+'kv_shape',T.INT64,[4],[0,0,kv,128])
            repeats=const(tag+'kv_indices',T.INT64,[heads],[j//(heads//kv) for j in range(heads)])
            k=node('Gather',[k,repeats],tag+'k_repeated',axis=2)
            v=node('Reshape',[v,kvshape],tag+'v_btkh')
            v=node('Gather',[v,repeats],tag+'v_repeated',axis=2)
        qshape=const(tag+'q_shape',T.INT64,[3],[0,0,heads*128])
        kshape=const(tag+'k_shape',T.INT64,[3],[0,0,(heads if key=='focus' else kv)*128])
        q=node('Reshape',[q,qshape],tag+'q_flat')
        k=node('Reshape',[k,kshape],tag+'k_flat')
        if key=='focus': v=node('Reshape',[v,qshape],tag+'v_flat')
        if not portable:
            q=node('Cast',[q],tag+'q16',to=T.FLOAT16)
            k=node('Cast',[k],tag+'k16',to=T.FLOAT16)
            v=node('Cast',[v],tag+'v16',to=T.FLOAT16)
        # The portable recipe writes FP32 attention straight to the original sink value.
        result=list(sink.output); require(len(result)==1,'attention sink differs')
        out=NS+tag+('raw32' if portable else 'raw16')
        if key=='focus':
            added.append(h.make_node('MultiHeadAttention',[q,k,v,'',packed],[out],name=NS+tag+'efficient',domain='com.microsoft',num_heads=heads,unidirectional=1,scale=scale))
            if portable: added.append(h.make_node('Where',[livequeries,out,zero],result,name=NS+tag+'zero_padded_queries'))
            else: out=node('Where',[livequeries,out,zero],tag+'zero_padded_queries')
        else:
            # CUDA GQA materializes both present tensors even with empty past inputs.
            outputs=[result[0] if portable else out,NS+tag+'present_key',NS+tag+'present_value']
            added.append(h.make_node('GroupQueryAttention',[q,k,v,'','',seqlens,total32],outputs,name=NS+tag+'efficient',domain='com.microsoft',num_heads=heads,kv_num_heads=kv,causal=1,local_window_size=-1,do_rotary=0,scale=scale))
        if not portable: added.append(h.make_node('Cast',[out],result,name=NS+tag+'output32',to=T.FLOAT))
        removed.add(sink.name)
    known={x.name for x in g.input}|{x.name for x in g.initializer}|{x.name for x in constants}
    ordered=reachable([n for n in g.node if n.name not in removed]+added,[x.name for x in g.output],known)
    require(not any(n.op_type=='Softmax' for n in ordered),'dense attention survived')
    retained={n.name:n for n in ordered}
    for n in original.graph.node:
        if n.name in retained and n.name not in modified:
            require(n.SerializeToString(deterministic=True)==retained[n.name].SerializeToString(deterministic=True),'protected math modified')
    require(all(t.name in {v for n in ordered for v in n.input} for t in g.initializer),'learned weight dropped')
    del g.node[:];g.node.extend(ordered);g.initializer.extend(constants)
    # Original intermediate dtype declarations describe FP32; replaced values receive fresh names.
    old_names={v for n in original.graph.node for v in n.output}
    saved=[v for v in g.value_info if v.name in old_names and v.name in {o for n in ordered for o in n.output}]
    del g.value_info[:];g.value_info.extend(saved)
    imports={x.domain:x.version for x in model.opset_import};require(imports.get('com.microsoft',1)==1,'custom opset differs')
    if 'com.microsoft' not in imports:model.opset_import.append(h.make_opsetid('com.microsoft',1))
    require([x.SerializeToString() for x in original.graph.input]==[x.SerializeToString() for x in g.input] and [x.SerializeToString() for x in original.graph.output]==[x.SerializeToString() for x in g.output], 'ABI mutation')
    if portable: return {'roles':roles,'scale_bits':scales,'recipe':'portable','arithmetic':'float32','qualification':False}
    return {'roles':roles,'scale_bits':scales,'required_native_sdpa_kernel':2 if key=='focus' else 1,'required_gemm_options':2,'qualification':False}

def source_bytes(tensor, directory):
    """Return a bounded source descriptor, never follow an external path alias."""
    length=math.prod(tensor.dims)*4
    if tensor.data_location==1:
        fields={x.key:x.value for x in tensor.external_data}
        require(len(fields)==len(tensor.external_data) and set(fields)<= {'location','offset','length','checksum'}, 'external fields differ')
        location=fields.get('location',''); p=Path(location)
        require(location not in ('','.','..') and not p.is_absolute() and len(p.parts)==1 and ':' not in location and '\\' not in location, 'unsafe external location')
        path=directory/p;require(not path.is_symlink() and not path.is_junction() and path.is_file() and path.resolve().parent==directory.resolve(), 'aliased external file')
        offset=int(fields.get('offset','0'));size=int(fields.get('length',str(length)))
        require(offset>=0 and size==length and offset+size<=path.stat().st_size,'external range differs')
        return path,offset,length,None
    require(not tensor.external_data and tensor.data_type==1,'unsupported inline tensor')
    if tensor.raw_data: raw=bytes(tensor.raw_data)
    else:raw=struct.pack('<'+'f'*len(tensor.float_data),*tensor.float_data)
    require(len(raw)==length,'inline tensor byte length differs')
    return None,0,length,raw

def convert_arrays(original, candidate, roles, directory, sidecar, np, deadline, clock=time.monotonic):
    """Stream actual arrays only when the owning future export runner invokes this.
    Caller MUST first authenticate all source files against original closure pins.
    This function never authenticates provenance by trusting its own output.
    """
    require(not sidecar.exists() and sidecar.parent.is_dir(), 'fresh sidecar required')
    source={t.name:t for t in original.graph.initializer}; dest={t.name:t for t in candidate.graph.initializer}
    descriptors={name:source_bytes(t,directory) for name,t in source.items()}
    ranges={}
    for node in candidate.graph.node:
        for attr in node.attribute:
            tensors=[attr.t] if attr.type==4 else list(attr.tensors) if attr.type==9 else []
            for t in tensors:
                if t.data_location==1:
                    require(t.data_type==1,'external Constant dtype differs')
                    path,offset,length,_=source_bytes(t,directory)
                    require(length<=4*1024*1024,'large unexpected Constant')
                    ranges.setdefault(path,[]).append((offset,offset+length,'Constant:'+node.name))
    for name,(path,offset,length,raw) in descriptors.items():
        if path is not None:ranges.setdefault(path,[]).append((offset,offset+length,name))
    for entries in ranges.values():
        entries.sort()
        require(all(a[1]<=b[0] for a,b in zip(entries,entries[1:])), 'overlapping external ranges')
    records=[]
    with sidecar.open('xb') as out:
        for name,src in source.items():
            require(clock()<deadline,'original export deadline expired')
            path,offset,length,raw=descriptors[name]; before=out.tell(); ih=hashlib.sha256();oh=hashlib.sha256();half=roles[name].endswith('_half')
            handle=path.open('rb') if path else None
            try:
                if handle:handle.seek(offset)
                pos=0
                while pos<length:
                    require(clock()<deadline,'original export deadline expired')
                    size=min(4*1024*1024,length-pos)
                    chunk=handle.read(size) if handle else raw[pos:pos+size]
                    require(len(chunk)==size,'short initializer read'); ih.update(chunk)
                    if half:data=half_chunk(chunk,np)
                    else:
                        finite_f32_chunk(chunk,np);data=chunk
                    out.write(data);oh.update(data);pos+=size
            finally:
                if handle:handle.close()
            t=dest[name];t.ClearField('raw_data');t.ClearField('float_data');t.ClearField('external_data');t.data_location=1
            for key,value in [('location',sidecar.name),('offset',str(before)),('length',str(out.tell()-before))]:
                entry=t.external_data.add();entry.key=key;entry.value=value
            records.append({'name':name,'role':roles[name],'shape':list(src.dims),'source_offset':offset,'source_bytes':length,'source_sha256':ih.hexdigest(),'output_offset':before,'output_bytes':out.tell()-before,'output_sha256':oh.hexdigest(),'source_dtype':'float32','output_dtype':'float16' if half else 'float32'})
    # Static Focus has external Constant tensor(s); retain exact small FP32 data,
    # never leave a candidate referencing an unsealed original external file.
    for node in candidate.graph.node:
        for attr in node.attribute:
            tensors=[attr.t] if attr.type==4 else list(attr.tensors) if attr.type==9 else []
            for t in tensors:
                if t.data_location==1:
                    require(t.data_type==1,'external Constant dtype differs')
                    path,offset,length,_=source_bytes(t,directory);require(length<=4*1024*1024,'large unexpected Constant')
                    require(clock()<deadline,'original export deadline expired')
                    with path.open('rb') as handle:handle.seek(offset);raw=handle.read(length)
                    require(len(raw)==length,'short Constant read')
                    t.ClearField('external_data');t.data_location=0;t.raw_data=raw
                    records.append({'constant':node.name,'source_sha256':sha(raw),'source_bytes':length,'retained_fp32':True})
    require(clock()<deadline,'original export deadline expired')
    return records


def export_after_owner_admission(source_path, key, source_file_pins, output, onnx, np, deadline, clock=time.monotonic, recipe='g5'):
    """Actual exporter API, held until Root's FP32 and native G5 admission.
    Both recipes write byte-identical weights.bin, because the array conversion
    depends only on the source and its role census.
    It does not create or consume a handwritten native qualification DTO. The
    existing owner must establish authority BEFORE invoking this function.
    source_file_pins is the original exporter closure's external-file SHA/size
    map, not a map synthesized from candidate arrays. Deadline is absolute.
    """
    require(key in ORIGINAL and clock()<deadline,'unknown model/expired original deadline')
    source_path=Path(source_path);directory=source_path.parent;output=Path(output)
    require(source_path.is_file() and not source_path.is_symlink(),'aliased/missing source graph')
    raw=source_path.read_bytes();require(sha(raw)==ORIGINAL[key],'not original full-FP32 graph')
    original=onnx.load_model_from_string(raw)
    # Never onnx.load(...load_external_data=True): all large arrays stay streamed.
    external=set()
    tensors=list(original.graph.initializer)
    for node in original.graph.node:
        for attr in node.attribute:
            tensors+=([attr.t] if attr.type==4 else list(attr.tensors) if attr.type==9 else [])
    for tensor in tensors:
        if tensor.data_location==1:
            fields={x.key:x.value for x in tensor.external_data}
            external.add(fields.get('location',''))
    require(set(source_file_pins)==external,'missing/extra original closure pins')
    def authenticate():
        for name,pin in sorted(source_file_pins.items()):
            require(clock()<deadline,'original export deadline expired')
            path=directory/name
            require(name not in ('','.','..') and len(Path(name).parts)==1 and ':' not in name and '\\' not in name and not path.is_symlink() and not path.is_junction() and path.is_file() and path.resolve().parent==directory.resolve(),'unsafe original closure location')
            require(set(pin)=={'sha256','bytes'} and path.stat().st_size==pin['bytes'],'source closure size differs')
            hasher=hashlib.sha256()
            with path.open('rb') as handle:
                while chunk:=handle.read(4*1024*1024):
                    require(clock()<deadline,'original export deadline expired');hasher.update(chunk)
            require(hasher.hexdigest()==pin['sha256'],'source closure digest differs')
    authenticate()
    candidate=copy.deepcopy(original);report=rewrite(candidate,key,onnx,recipe)
    require(clock()<deadline,'original export deadline expired')
    require(not output.exists() and output.parent.is_dir() and not output.parent.is_symlink() and not output.parent.is_junction(),'fresh approved output directory required')
    output.mkdir(exist_ok=False)
    arrays=convert_arrays(original,candidate,report['roles'],directory,output/'weights.bin',np,deadline,clock)
    authenticate();require(sha(source_path.read_bytes())==ORIGINAL[key],'original graph changed')
    require(clock()<deadline,'original export deadline expired')
    # Checker validates schema without loading large data; it does not establish
    # numerical correctness, CUDA placement or actual Flash/MEA dispatch.
    target=output/'model.onnx'
    with target.open('xb') as handle:handle.write(candidate.SerializeToString(deterministic=True))
    onnx.checker.check_model(str(target),full_check=False)
    require(clock()<deadline,'original export deadline expired')
    report.update({'source_graph':ORIGINAL[key],'source_files':source_file_pins,'initializers':arrays,'graph_sha256':sha(target.read_bytes()),'qualification':False,'native_execution':False})
    return report

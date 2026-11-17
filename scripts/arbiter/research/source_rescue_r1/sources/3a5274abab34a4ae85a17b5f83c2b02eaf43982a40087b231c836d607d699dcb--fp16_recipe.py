# SPDX-License-Identifier: Apache-2.0
"""G5 preparation helpers only: no model/runtime imports, file transforms or dispatch."""
import math,struct
MODELS={'focus':(28,16,8,512),'compact':(28,16,8,8192),'large':(36,32,8,8192)}
def half_bytes_from_f32(source):
    if len(source)%4:raise ValueError('unaligned source FP32 initializer')
    out=bytearray()
    for (value,) in struct.iter_unpack('<f',source):
        if not math.isfinite(value):raise ValueError('nonfinite source weight')
        try:encoded=struct.pack('<e',value)
        except OverflowError as exc:raise ValueError('finite FP32 weight overflows binary16') from exc
        if not math.isfinite(struct.unpack('<e',encoded)[0]):raise ValueError('half conversion nonfinite')
        out.extend(encoded)
    return bytes(out)
def mask_recipe(model,mask,positions):
    if model not in MODELS:raise ValueError('unselected model')
    t=len(mask)
    if not t or len(positions)!=t or any(type(x) is not int or x not in (0,1) for x in mask):raise ValueError('invalid binary mask/position shape')
    if model=='focus':
        if t!=512 or not any(mask):raise ValueError('Focus requires512 and a live token')
        p=mask.index(1);live=t-p
        if mask!=[0]*p+[1]*live:raise ValueError('padding holes/right padding unsupported')
        if positions!=list(range(t)):raise ValueError('native Focus position rule differs')
        return {'operator':'MultiHeadAttention','unidirectional':1,'sdpa_kernel':2,'packed_mask':[live,p,t,p,t],'query_start':p,'key_start':p,'length':live,'attention_bias':False}
    if t>8192 or mask!=[1]*t or positions!=list(range(t)):raise ValueError('Context requires all-live sequential native input')
    return {'operator':'GroupQueryAttention','causal':1,'sdpa_kernel':1,'seqlens_k':[t-1],'total_sequence_length':[t],'query_start':0,'key_start':0,'length':t,'attention_bias':False,'do_rotary':0}
def source_key_for_query(recipe,query_index):
    start=recipe['query_start'];length=recipe['length']
    if not start<=query_index<start+length:raise ValueError('padded query has no promised output')
    relative=query_index-start
    return tuple(range(recipe['key_start'],recipe['key_start']+relative+1))
def grouped_kv_head(query_head,query_heads,kv_heads):
    if query_heads<=0 or kv_heads<=0 or query_heads%kv_heads or not 0<=query_head<query_heads:raise ValueError('invalid grouped heads')
    return query_head//(query_heads//kv_heads)
def require_dispatch(records,expected_names,backend):
    # Records must be parsed by the native observer from actual owned stdout, not supplied claims.
    if backend not in ('FLASH_ATTENTION','EFFICIENT_ATTENTION'):raise ValueError('unsupported planned backend')
    seen={}
    for name,dtype,actual in records:
        if name not in expected_names or name in seen or dtype!='fp16' or actual!=backend:raise ValueError('incomplete/foreign/duplicate/math dispatch')
        seen[name]=actual
    if set(seen)!=set(expected_names):raise ValueError('missing actual attention dispatch')
